use std::sync::{OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

wit_bindgen::generate!({
    world: "commands",
    path: "wit",
    generate_all
});

use crate::exports::pato::pato_commands::platform;
use crate::exports::pato::plugin;
use crate::pato::pato_commands::{command_events, signals};
use crate::pato::pato_twitch::chat;
use pato::plugin::storage;

mod dispatch;
mod draft;
// The editing API (`State::upsert` etc.) has no caller until the widget and
// plugin-registration steps; it is covered by its own tests.
#[allow(dead_code)]
mod model;
mod parser;
mod widgets;

const KEY_STATE: &str = "state";

static PLUGIN: OnceLock<RwLock<PatoCommands>> = OnceLock::new();

fn with_plugin<F, R>(f: F) -> R
where
    F: FnOnce(&mut PatoCommands) -> R,
{
    let plugin = PLUGIN.get().unwrap();
    let mut plugin = plugin.write().unwrap();
    f(&mut plugin)
}

struct PatoCommands {
    state: model::State,
    cooldowns: dispatch::Cooldowns,
    widget: widgets::CommandsWidget,
}

impl PatoCommands {
    fn init() -> Self {
        let state = storage::get(KEY_STATE)
            .and_then(|json| match serde_json::from_str(&json) {
                Ok(state) => Some(state),
                Err(e) => {
                    eprintln!("pato-commands: ignoring unreadable saved state: {e}");
                    None
                }
            })
            .unwrap_or_default();
        PatoCommands {
            widget: widgets::CommandsWidget::new(&state),
            state,
            cooldowns: dispatch::Cooldowns::default(),
        }
    }

    fn save(&self) {
        match serde_json::to_string(&self.state) {
            Ok(json) => {
                storage::set(KEY_STATE, &json);
            }
            Err(e) => eprintln!("pato-commands: failed to save state: {e}"),
        }
    }
}

impl plugin::init::Guest for PatoCommands {
    fn init() -> Result<bool, String> {
        PLUGIN
            .set(RwLock::new(PatoCommands::init()))
            .map_err(|_| "Failed to set plugin")?;
        Ok(true)
    }
}

impl plugin::widget_events::Guest for PatoCommands {
    // No pre-made status widget.
    fn on_status_event(_widget_id: String, _action: String) {}

    fn on_event(event: plugin::widget_events::WidgetEvent) {
        with_plugin(|p| {
            if p.widget.on_event(&event, &mut p.state) {
                p.save();
            }
        });
    }

    // The widget is responsive; nothing to do on resize.
    fn on_layout(_layout: plugin::widget_events::WidgetLayout) {}

    fn on_modal_closed(id: String) {
        with_plugin(|p| p.widget.on_modal_closed(&id));
    }

    fn on_confirm(id: String, confirmed: bool) {
        with_plugin(|p| {
            if p.widget.on_confirm(&id, confirmed, &mut p.state) {
                p.save();
            }
        });
    }
}

// Chat lines arrive here from platform plugins.
impl platform::Guest for PatoCommands {
    fn ingest(message: platform::ChatMessage) {
        let role = from_wit_role(message.role);
        let msg = dispatch::Message {
            platform: &message.platform,
            user_id: &message.user_id,
            username: &message.username,
            display_name: &message.display_name,
            role,
            text: &message.text,
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        // Decide under the lock, act after releasing it: replying and
        // signalling both call out through the host.
        let outcome = with_plugin(|p| dispatch::handle(&p.state, &mut p.cooldowns, &msg, now));
        match outcome {
            dispatch::Outcome::Ignore => {}
            dispatch::Outcome::Reply(text) => reply(&message.platform, &text),
            dispatch::Outcome::Trigger { owner, invocation } => {
                signals::emit(&owner, &to_wit_invocation(invocation));
            }
        }
    }
}

/// Sends `text` back to `platform`. Only Twitch has a send API so far.
fn reply(platform: &str, text: &str) {
    match platform {
        "twitch" => {
            if !chat::send_message(text) {
                eprintln!("pato-commands: Twitch chat isn't connected; dropped reply");
            }
        }
        other => eprintln!("pato-commands: no way to reply on platform \"{other}\""),
    }
}

fn from_wit_role(role: platform::Role) -> model::Role {
    match role {
        platform::Role::Everyone => model::Role::Everyone,
        platform::Role::Subscriber => model::Role::Subscriber,
        platform::Role::Vip => model::Role::Vip,
        platform::Role::Moderator => model::Role::Moderator,
        platform::Role::Broadcaster => model::Role::Broadcaster,
    }
}

fn to_wit_role(role: model::Role) -> command_events::Role {
    match role {
        model::Role::Everyone => command_events::Role::Everyone,
        model::Role::Subscriber => command_events::Role::Subscriber,
        model::Role::Vip => command_events::Role::Vip,
        model::Role::Moderator => command_events::Role::Moderator,
        model::Role::Broadcaster => command_events::Role::Broadcaster,
    }
}

fn to_wit_invocation(i: dispatch::Invocation) -> command_events::Invocation {
    command_events::Invocation {
        command: i.command,
        platform: i.platform,
        user_id: i.user_id,
        username: i.username,
        display_name: i.display_name,
        role: to_wit_role(i.role),
        args: i
            .args
            .into_iter()
            .map(|(name, value)| {
                let value = match value {
                    parser::Value::Text(s) => command_events::ArgValue::Text(s),
                    parser::Value::Num(n) => command_events::ArgValue::Num(n),
                    parser::Value::Flag(b) => command_events::ArgValue::Flag(b),
                };
                (name, value)
            })
            .collect(),
    }
}

export!(PatoCommands);
