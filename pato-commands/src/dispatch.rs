//! Turns one incoming chat line into an outcome: ignore it, reply in chat, or
//! signal a plugin. Pure (the caller supplies the clock), so the whole
//! pipeline is unit-tested natively.
//!
//! Order of checks for a line: master switch -> prefix -> command lookup -> enabled -> role ->
//! cooldown -> argument parsing. Anything failing before parsing is ignored
//! silently (replying "you can't do that" to every chatter is spam); a
//! parse failure replies with the error and usage, since the user was
//! allowed to run the command and can fix it. Cooldowns only start once the
//! arguments parsed, so a typo doesn't burn the cooldown.

use std::collections::HashMap;

use crate::model::{Command, Kind, Owner, Role, State, MAX_COOLDOWN_SECS, MAX_RESPONSE_LEN};
use crate::parser::{self, Value};

/// A chat line, in the shape platforms hand it over.
pub struct Message<'a> {
    pub platform: &'a str,
    pub user_id: &'a str,
    pub username: &'a str,
    pub display_name: &'a str,
    pub role: Role,
    pub text: &'a str,
}

/// What a trigger command's owner is told.
#[derive(Debug, PartialEq)]
pub struct Invocation {
    /// Canonical command name (not the alias that was typed).
    pub command: String,
    pub platform: String,
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub args: Vec<(String, Value)>,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Ignore,
    /// Send this text back to the platform the line came from.
    Reply(String),
    /// Signal the plugin that registered the command.
    Trigger { owner: String, invocation: Invocation },
}

/// When each command (and each user of it) last ran, in ms since the epoch.
#[derive(Default)]
pub struct Cooldowns {
    global: HashMap<String, u64>,
    user: HashMap<(String, String, String), u64>,
}

const PRUNE_ABOVE: usize = 2048;

impl Cooldowns {
    fn blocked(&self, cmd: &Command, platform: &str, username: &str, now: u64) -> bool {
        let still = |last: Option<&u64>, secs: u32| {
            secs > 0 && last.is_some_and(|last| now < last.saturating_add(u64::from(secs) * 1000))
        };
        still(self.global.get(&cmd.name), cmd.global_cooldown_secs)
            || still(self.user.get(&Self::user_key(cmd, platform, username)), cmd.user_cooldown_secs)
    }

    fn start(&mut self, cmd: &Command, platform: &str, username: &str, now: u64) {
        if cmd.global_cooldown_secs > 0 {
            self.global.insert(cmd.name.clone(), now);
        }
        if cmd.user_cooldown_secs > 0 {
            self.user.insert(Self::user_key(cmd, platform, username), now);
        }
        // Entries older than the longest allowed cooldown can never block
        // again; drop them so a busy chat can't grow this without bound.
        if self.user.len() > PRUNE_ABOVE {
            let horizon = now.saturating_sub(u64::from(MAX_COOLDOWN_SECS) * 1000);
            self.user.retain(|_, last| *last >= horizon);
        }
    }

    fn user_key(cmd: &Command, platform: &str, username: &str) -> (String, String, String) {
        (cmd.name.clone(), platform.to_string(), username.to_lowercase())
    }
}

pub fn handle(state: &State, cooldowns: &mut Cooldowns, msg: &Message, now_ms: u64) -> Outcome {
    if !state.enabled {
        return Outcome::Ignore;
    }
    let Some(prefix) = state.prefixes.get(msg.platform) else {
        return Outcome::Ignore;
    };
    let Some(rest) = msg.text.trim_start().strip_prefix(prefix.as_str()) else {
        return Outcome::Ignore;
    };
    // Name is the first word; `args` is everything after it, untouched.
    let (name, args) = match rest.find(char::is_whitespace) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if name.is_empty() {
        return Outcome::Ignore;
    }
    let Some(cmd) = state.find(name) else {
        return Outcome::Ignore;
    };
    if !cmd.enabled || msg.role < cmd.min_role {
        return Outcome::Ignore;
    }
    let bypass = cmd.bypass_cooldown && msg.role >= Role::Moderator;
    if !bypass && cooldowns.blocked(cmd, msg.platform, msg.username, now_ms) {
        return Outcome::Ignore;
    }
    let Ok(parts) = parser::validate_template(&cmd.args) else {
        // Can't happen for a command that went through `State::upsert`.
        return Outcome::Ignore;
    };
    let parsed = match parser::parse_args(&parts, args) {
        Ok(parsed) => parsed,
        Err(error) => {
            let usage = [format!("{prefix}{}", cmd.name)]
                .into_iter()
                .chain(parts.iter().map(parser::Part::literal))
                .collect::<Vec<_>>()
                .join(" ");
            return Outcome::Reply(format!("{}: {error}. Usage: {usage}", msg.display_name));
        }
    };
    if !bypass {
        cooldowns.start(cmd, msg.platform, msg.username, now_ms);
    }
    match (&cmd.kind, &cmd.owner) {
        (Kind::Info { response }, _) => Outcome::Reply(render(response, msg, &parsed)),
        (Kind::Trigger, Owner::Plugin(owner)) => Outcome::Trigger {
            owner: owner.clone(),
            invocation: Invocation {
                command: cmd.name.clone(),
                platform: msg.platform.to_string(),
                user_id: msg.user_id.to_string(),
                username: msg.username.to_string(),
                display_name: msg.display_name.to_string(),
                role: msg.role,
                args: parsed,
            },
        },
        // A trigger with no plugin owner can't exist (see `State::upsert`).
        (Kind::Trigger, Owner::User) => Outcome::Ignore,
    }
}

/// Substitutes `{user}`, `{username}`, `{platform}` and argument names into a
/// response. Unknown `{words}` are left as written.
fn render(response: &str, msg: &Message, args: &[(String, Value)]) -> String {
    let lookup = |var: &str| -> Option<String> {
        match var {
            "user" => Some(msg.display_name.to_string()),
            "username" => Some(msg.username.to_string()),
            "platform" => Some(msg.platform.to_string()),
            _ => args.iter().find(|(k, _)| k == var).map(|(_, v)| match v {
                Value::Text(s) => s.clone(),
                Value::Num(n) => n.to_string(),
                Value::Flag(b) => b.to_string(),
            }),
        }
    };
    let mut out = String::new();
    let mut rest = response;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let replaced = after.find('}').and_then(|close| {
            let value = lookup(&after[..close])?;
            Some((value, close))
        });
        match replaced {
            Some((value, close)) => {
                out.push_str(&value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    // Substituted user text can push it past the platform's limit.
    out.chars().take(MAX_RESPONSE_LEN).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{info, trigger};

    fn msg<'a>(text: &'a str, role: Role) -> Message<'a> {
        Message {
            platform: "twitch",
            user_id: "7",
            username: "bob",
            display_name: "Bob",
            role,
            text,
        }
    }

    fn state(commands: Vec<Command>) -> State {
        let mut s = State::default();
        for c in commands {
            s.upsert(c).unwrap();
        }
        s
    }

    #[test]
    fn master_switch_off_ignores_everything() {
        let mut s = state(vec![info("hi", "hello")]);
        s.enabled = false;
        assert_eq!(run(&s, "!hi", Role::Broadcaster), Outcome::Ignore);
        s.enabled = true;
        assert_eq!(run(&s, "!hi", Role::Broadcaster), reply("hello"));
    }

    fn run(s: &State, text: &str, role: Role) -> Outcome {
        handle(s, &mut Cooldowns::default(), &msg(text, role), 0)
    }

    fn reply(text: &str) -> Outcome {
        Outcome::Reply(text.to_string())
    }

    #[test]
    fn non_commands_are_ignored() {
        let s = state(vec![info("hi", "hello")]);
        for text in ["hi", "hello !hi", "!", "! hi", "!nope", "", "   "] {
            assert_eq!(run(&s, text, Role::Everyone), Outcome::Ignore, "{text:?}");
        }
    }

    #[test]
    fn platform_without_a_prefix_never_triggers() {
        let s = state(vec![info("hi", "hello")]);
        let m = Message { platform: "discord", ..msg("!hi", Role::Everyone) };
        assert_eq!(handle(&s, &mut Cooldowns::default(), &m, 0), Outcome::Ignore);
    }

    #[test]
    fn each_platform_uses_its_own_prefix() {
        let mut s = state(vec![info("hi", "hello")]);
        s.set_prefix("discord", "/").unwrap();
        let m = |text| Message { platform: "discord", ..msg(text, Role::Everyone) };
        assert_eq!(handle(&s, &mut Cooldowns::default(), &m("/hi"), 0), reply("hello"));
        assert_eq!(handle(&s, &mut Cooldowns::default(), &m("!hi"), 0), Outcome::Ignore);
    }

    #[test]
    fn matches_names_and_aliases_case_insensitively() {
        let mut c = info("socials", "links!");
        c.aliases = vec!["links".into()];
        let s = state(vec![c]);
        for text in ["!socials", "!SOCIALS", "!links", "  !Links  "] {
            assert_eq!(run(&s, text, Role::Everyone), reply("links!"), "{text:?}");
        }
    }

    #[test]
    fn multi_character_prefix() {
        let mut s = state(vec![info("hi", "hello")]);
        s.set_prefix("twitch", "pato!").unwrap();
        assert_eq!(run(&s, "pato!hi", Role::Everyone), reply("hello"));
        assert_eq!(run(&s, "!hi", Role::Everyone), Outcome::Ignore);
    }

    #[test]
    fn disabled_commands_are_ignored() {
        let mut c = info("hi", "hello");
        c.enabled = false;
        assert_eq!(run(&state(vec![c]), "!hi", Role::Broadcaster), Outcome::Ignore);
    }

    #[test]
    fn role_gate_uses_the_ladder_and_fails_silently() {
        let mut c = info("shoutout", "hey");
        c.min_role = Role::Vip;
        let s = state(vec![c]);
        assert_eq!(run(&s, "!shoutout", Role::Everyone), Outcome::Ignore);
        assert_eq!(run(&s, "!shoutout", Role::Subscriber), Outcome::Ignore);
        assert_eq!(run(&s, "!shoutout", Role::Vip), reply("hey"));
        assert_eq!(run(&s, "!shoutout", Role::Moderator), reply("hey"));
        assert_eq!(run(&s, "!shoutout", Role::Broadcaster), reply("hey"));
    }

    #[test]
    fn info_substitutes_user_and_arguments() {
        let mut c = info("hug", "{user} ({username} on {platform}) hugs {target} x{n}");
        c.args = "<target:string> [n:number]".into();
        let s = state(vec![c]);
        assert_eq!(
            run(&s, "!hug alice 3", Role::Everyone),
            reply("Bob (bob on twitch) hugs alice x3")
        );
    }

    #[test]
    fn unfilled_optional_placeholder_is_left_visible_rather_than_dropped() {
        // Documented behaviour: an omitted optional argument stays as typed.
        let mut c = info("hug", "hugs {target}");
        c.args = "[target:string]".into();
        assert_eq!(run(&state(vec![c]), "!hug", Role::Everyone), reply("hugs {target}"));
    }

    #[test]
    fn numbers_render_without_trailing_zero_and_bools_as_words() {
        let mut c = info("x", "{n} {b} {f}");
        c.args = "<n:number> <b:boolean> <f:number>".into();
        assert_eq!(run(&state(vec![c]), "!x 10 true 2.5", Role::Everyone), reply("10 true 2.5"));
    }

    #[test]
    fn user_text_is_not_re_expanded() {
        // A chatter can't smuggle placeholders in through an argument.
        let mut c = info("say", "{msg}");
        c.args = "<msg:string...>".into();
        assert_eq!(run(&state(vec![c]), "!say {user}", Role::Everyone), reply("{user}"));
    }

    #[test]
    fn substituted_reply_is_capped_to_the_platform_limit() {
        let mut c = info("say", "{msg}");
        c.args = "<msg:string...>".into();
        let long = format!("!say {}", "a".repeat(2000));
        let Outcome::Reply(text) = run(&state(vec![c]), &long, Role::Everyone) else { panic!() };
        assert_eq!(text.chars().count(), MAX_RESPONSE_LEN);
    }

    #[test]
    fn bad_arguments_reply_with_error_and_usage() {
        let mut c = info("give", "ok");
        c.args = "<who:string> <amount:number> [note:string...]".into();
        let s = state(vec![c]);
        assert_eq!(
            run(&s, "!give bob lots", Role::Everyone),
            reply("Bob: Expected <amount> to be a number. Usage: !give <who:string> <amount:number> [note:string...]")
        );
        assert_eq!(
            run(&s, "!give", Role::Everyone),
            reply("Bob: Missing <who>. Usage: !give <who:string> <amount:number> [note:string...]")
        );
    }

    #[test]
    fn usage_is_not_shown_to_people_who_cannot_run_the_command() {
        let mut c = info("give", "ok");
        c.args = "<who:string>".into();
        c.min_role = Role::Moderator;
        assert_eq!(run(&state(vec![c]), "!give", Role::Everyone), Outcome::Ignore);
    }

    #[test]
    fn trigger_signals_the_owner_with_parsed_args_and_canonical_name() {
        let mut c = trigger("scene", "pato_obs");
        c.aliases = vec!["sc".into()];
        c.args = "<name:string...>".into();
        c.min_role = Role::Moderator;
        let s = state(vec![c]);
        assert_eq!(run(&s, "!scene Just Chatting", Role::Everyone), Outcome::Ignore);
        assert_eq!(
            run(&s, "!SC Just  Chatting", Role::Moderator),
            Outcome::Trigger {
                owner: "pato_obs".into(),
                invocation: Invocation {
                    command: "scene".into(),
                    platform: "twitch".into(),
                    user_id: "7".into(),
                    username: "bob".into(),
                    display_name: "Bob".into(),
                    role: Role::Moderator,
                    args: vec![("name".into(), Value::Text("Just  Chatting".into()))],
                }
            }
        );
    }

    // --- cooldowns --------------------------------------------------------

    fn at(s: &State, cd: &mut Cooldowns, who: &str, role: Role, now: u64) -> Outcome {
        let m = Message { username: who, display_name: who, ..msg("!hi", role) };
        handle(s, cd, &m, now)
    }

    #[test]
    fn global_cooldown_blocks_everyone_until_it_expires() {
        let mut c = info("hi", "hello");
        c.global_cooldown_secs = 10;
        let s = state(vec![c]);
        let mut cd = Cooldowns::default();
        assert_eq!(at(&s, &mut cd, "a", Role::Everyone, 1_000), reply("hello"));
        assert_eq!(at(&s, &mut cd, "b", Role::Everyone, 10_999), Outcome::Ignore);
        assert_eq!(at(&s, &mut cd, "b", Role::Everyone, 11_000), reply("hello"));
    }

    #[test]
    fn user_cooldown_is_per_user_and_case_insensitive() {
        let mut c = info("hi", "hello");
        c.user_cooldown_secs = 10;
        let s = state(vec![c]);
        let mut cd = Cooldowns::default();
        assert_eq!(at(&s, &mut cd, "a", Role::Everyone, 0), reply("hello"));
        assert_eq!(at(&s, &mut cd, "A", Role::Everyone, 5_000), Outcome::Ignore);
        assert_eq!(at(&s, &mut cd, "b", Role::Everyone, 5_000), reply("hello"));
        assert_eq!(at(&s, &mut cd, "a", Role::Everyone, 10_000), reply("hello"));
    }

    #[test]
    fn cooldowns_are_per_command_and_per_platform() {
        let mut a = info("a", "A");
        a.user_cooldown_secs = 60;
        let s = state(vec![a, info("b", "B")]);
        let mut cd = Cooldowns::default();
        let say = |cd: &mut Cooldowns, platform, text, now| {
            let m = Message { platform, ..msg(text, Role::Everyone) };
            handle(&s, cd, &m, now)
        };
        let mut s2 = s.clone();
        s2.set_prefix("discord", "!").unwrap();
        assert_eq!(say(&mut cd, "twitch", "!a", 0), reply("A"));
        assert_eq!(say(&mut cd, "twitch", "!a", 1), Outcome::Ignore);
        assert_eq!(say(&mut cd, "twitch", "!b", 1), reply("B"));
        // `s` has no discord prefix; use s2 for the cross-platform check.
        let m = Message { platform: "discord", ..msg("!a", Role::Everyone) };
        assert_eq!(handle(&s2, &mut cd, &m, 2), reply("A"));
    }

    #[test]
    fn moderators_and_broadcaster_bypass_cooldowns_and_do_not_start_them() {
        let mut c = info("hi", "hello");
        c.global_cooldown_secs = 60;
        let s = state(vec![c]);
        let mut cd = Cooldowns::default();
        assert_eq!(at(&s, &mut cd, "m", Role::Moderator, 0), reply("hello"));
        assert_eq!(at(&s, &mut cd, "m", Role::Broadcaster, 1), reply("hello"));
        // Their use didn't lock out a regular chatter...
        assert_eq!(at(&s, &mut cd, "a", Role::Everyone, 2), reply("hello"));
        // ...whose own use does.
        assert_eq!(at(&s, &mut cd, "m", Role::Vip, 3), Outcome::Ignore);
    }

    #[test]
    fn bypass_can_be_turned_off_per_command() {
        let mut c = info("hi", "hello");
        c.global_cooldown_secs = 60;
        c.bypass_cooldown = false;
        let s = state(vec![c]);
        let mut cd = Cooldowns::default();
        assert_eq!(at(&s, &mut cd, "m", Role::Moderator, 0), reply("hello"));
        assert_eq!(at(&s, &mut cd, "m", Role::Broadcaster, 1), Outcome::Ignore);
    }

    #[test]
    fn a_failed_parse_does_not_start_the_cooldown() {
        let mut c = info("pay", "paid {n}");
        c.args = "<n:number>".into();
        c.user_cooldown_secs = 60;
        let s = state(vec![c]);
        let mut cd = Cooldowns::default();
        let say = |cd: &mut Cooldowns, text, now| handle(&s, cd, &msg(text, Role::Everyone), now);
        assert!(matches!(say(&mut cd, "!pay x", 0), Outcome::Reply(r) if r.contains("Expected <n>")));
        assert_eq!(say(&mut cd, "!pay 5", 1), reply("paid 5"));
        assert_eq!(say(&mut cd, "!pay 6", 2), Outcome::Ignore);
    }

    #[test]
    fn a_blocked_command_stays_silent_even_with_bad_arguments() {
        let mut c = info("pay", "ok");
        c.args = "<n:number>".into();
        c.global_cooldown_secs = 60;
        let s = state(vec![c]);
        let mut cd = Cooldowns::default();
        assert_eq!(handle(&s, &mut cd, &msg("!pay 1", Role::Everyone), 0), reply("ok"));
        assert_eq!(handle(&s, &mut cd, &msg("!pay x", Role::Everyone), 1), Outcome::Ignore);
    }

    #[test]
    fn user_cooldown_map_is_pruned_when_it_grows_large() {
        let mut c = info("hi", "hello");
        c.user_cooldown_secs = 1;
        let s = state(vec![c]);
        let mut cd = Cooldowns::default();
        for i in 0..=PRUNE_ABOVE {
            at(&s, &mut cd, &format!("u{i}"), Role::Everyone, 0);
        }
        assert!(cd.user.len() > PRUNE_ABOVE);
        // A day and a bit later, the next start() sweeps the stale ones.
        at(&s, &mut cd, "late", Role::Everyone, (u64::from(MAX_COOLDOWN_SECS) + 10) * 1000);
        assert_eq!(cd.user.len(), 1);
    }
}
