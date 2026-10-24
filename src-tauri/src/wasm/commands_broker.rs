//! Host broker for pato-commands: its public `platform` API (platform plugins
//! call in via `ingest`) and the `signals` import (pato-commands calls out via
//! `emit`, which this forwards to the plugin owning a trigger command's
//! `command-events.on-command`). Same mechanism as `obs_broker.rs`, with one
//! difference: both directions are fire-and-forget.
//!
//! Plugins process calls strictly one at a time on their own thread. If
//! `ingest` waited for pato-commands, a platform plugin would be blocked
//! mid-call while pato-commands handled the message - and a command reply
//! calls back into that same platform plugin (e.g. `pato-twitch/chat.
//! send-message`), which could then never be serviced: deadlock. So the
//! forward runs as a spawned task on the calling plugin's runtime and
//! `ingest` returns immediately.

use wasmtime::component::{HasSelf, Val};

use crate::wasm;

mod plugin {
    wasmtime::component::bindgen!({
        path: "wit-broker",
        world: "commands-broker",
        imports: { default: async },
    });
}
use plugin::pato::pato_commands::{command_events, platform, signals};

const PATO_COMMANDS: &str = "pato_commands";
const PLATFORM: &str = "pato:pato-commands/platform@0.1.0";
const COMMAND_EVENTS: &str = "pato:pato-commands/command-events@0.1.0";

fn role_name(role: &command_events::Role) -> &'static str {
    match role {
        command_events::Role::Everyone => "everyone",
        command_events::Role::Subscriber => "subscriber",
        command_events::Role::Vip => "vip",
        command_events::Role::Moderator => "moderator",
        command_events::Role::Broadcaster => "broadcaster",
    }
}

fn to_val(m: &platform::ChatMessage) -> Val {
    Val::Record(vec![
        ("platform".into(), Val::String(m.platform.clone())),
        ("user-id".into(), Val::String(m.user_id.clone())),
        ("username".into(), Val::String(m.username.clone())),
        ("display-name".into(), Val::String(m.display_name.clone())),
        ("role".into(), Val::Enum(role_name(&m.role).into())),
        ("text".into(), Val::String(m.text.clone())),
    ])
}

fn arg_value_val(v: &command_events::ArgValue) -> Val {
    let (case, payload) = match v {
        command_events::ArgValue::Text(s) => ("text", Val::String(s.clone())),
        command_events::ArgValue::Num(n) => ("num", Val::Float64(*n)),
        command_events::ArgValue::Flag(b) => ("flag", Val::Bool(*b)),
    };
    Val::Variant(case.into(), Some(Box::new(payload)))
}

/// Field order matches the WIT record; wasmtime checks records by position.
fn invocation_val(i: &command_events::Invocation) -> Val {
    Val::Record(vec![
        ("command".into(), Val::String(i.command.clone())),
        ("platform".into(), Val::String(i.platform.clone())),
        ("user-id".into(), Val::String(i.user_id.clone())),
        ("username".into(), Val::String(i.username.clone())),
        ("display-name".into(), Val::String(i.display_name.clone())),
        ("role".into(), Val::Enum(role_name(&i.role).into())),
        (
            "args".into(),
            Val::List(
                i.args
                    .iter()
                    .map(|(name, value)| Val::Tuple(vec![Val::String(name.clone()), arg_value_val(value)]))
                    .collect(),
            ),
        ),
    ])
}

impl signals::Host for wasm::HostData {
    async fn emit(&mut self, owner: String, invocation: command_events::Invocation) {
        let val = invocation_val(&invocation);
        tokio::spawn(async move {
            // Owner not installed / not exporting `command-events` is
            // reported by the plugin thread or `plugin_call_by_name`'s error;
            // nothing more to do here.
            if let Err(e) = wasm::plugin_call_by_name(&owner, COMMAND_EVENTS, "on-command", vec![val], vec![]).await {
                eprintln!("pato-commands: couldn't signal plugin \"{owner}\": {e}");
            }
        });
    }
}

impl platform::Host for wasm::HostData {
    async fn ingest(&mut self, message: platform::ChatMessage) {
        let val = to_val(&message);
        tokio::spawn(async move {
            // Errors (e.g. pato-commands not installed) are already reported
            // by the plugin thread, or are expected; nothing to do here.
            let _ = wasm::plugin_call_by_name(PATO_COMMANDS, PLATFORM, "ingest", vec![val], vec![]).await;
        });
    }
}

pub fn init() -> Result<(), String> {
    println!("Initializing commands broker module...");
    wasm::link(|linker| {
        platform::add_to_linker::<_, HasSelf<_>>(linker, |host| host)?;
        signals::add_to_linker::<_, HasSelf<_>>(linker, |host| host)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_message_with_wit_field_names_and_enum_role() {
        let m = platform::ChatMessage {
            platform: "twitch".into(),
            user_id: "1".into(),
            username: "a".into(),
            display_name: "A".into(),
            role: platform::Role::Vip,
            text: "!hi".into(),
        };
        let Val::Record(fields) = to_val(&m) else { panic!("not a record") };
        let names: Vec<_> = fields.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["platform", "user-id", "username", "display-name", "role", "text"]);
        assert!(matches!(&fields[4].1, Val::Enum(r) if r == "vip"));
    }

    #[test]
    fn encodes_an_invocation_with_typed_args() {
        let i = command_events::Invocation {
            command: "scene".into(),
            platform: "twitch".into(),
            user_id: "1".into(),
            username: "a".into(),
            display_name: "A".into(),
            role: command_events::Role::Moderator,
            args: vec![
                ("name".into(), command_events::ArgValue::Text("Main".into())),
                ("secs".into(), command_events::ArgValue::Num(2.5)),
                ("on".into(), command_events::ArgValue::Flag(true)),
            ],
        };
        let Val::Record(fields) = invocation_val(&i) else { panic!("not a record") };
        let names: Vec<_> = fields.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["command", "platform", "user-id", "username", "display-name", "role", "args"]);
        let Val::List(args) = &fields[6].1 else { panic!("args not a list") };
        let cases: Vec<_> = args
            .iter()
            .map(|a| match a {
                Val::Tuple(t) => match &t[1] {
                    Val::Variant(case, Some(p)) => (case.clone(), matches!(**p, Val::String(_) | Val::Float64(_) | Val::Bool(_))),
                    _ => panic!("not a variant"),
                },
                _ => panic!("not a tuple"),
            })
            .collect();
        assert_eq!(cases, [("text".into(), true), ("num".into(), true), ("flag".into(), true)]);
    }
}
