//! Command definitions and the persisted state that holds them.
//!
//! Pure data and validation, no wasm/WIT types, so it is unit-tested natively.
//! Everything here is normalised and validated on the way in (`State::upsert`),
//! so dispatch can trust what it reads.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::parser;

pub const MAX_NAME_LEN: usize = 32;
/// Twitch's chat message limit; other platforms are at least this generous.
pub const MAX_RESPONSE_LEN: usize = 500;
/// Cooldowns above a day are almost certainly a typo.
pub const MAX_COOLDOWN_SECS: u32 = 86_400;
/// Names usable as `{name}` in a response without being an argument.
pub const RESERVED_VARS: [&str; 3] = ["user", "username", "platform"];

/// A chatter's standing, lowest to highest. Mirrors the WIT `role` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Everyone,
    Subscriber,
    Vip,
    Moderator,
    Broadcaster,
}

impl Role {
    pub const ALL: [Role; 5] = [
        Role::Everyone,
        Role::Subscriber,
        Role::Vip,
        Role::Moderator,
        Role::Broadcaster,
    ];

    /// Stable identifier, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Everyone => "everyone",
            Role::Subscriber => "subscriber",
            Role::Vip => "vip",
            Role::Moderator => "moderator",
            Role::Broadcaster => "broadcaster",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.as_str() == s)
    }

    /// For the UI. Roles are a ladder, so everything above the named role
    /// can run the command too.
    pub fn label(self) -> &'static str {
        match self {
            Role::Everyone => "Anyone",
            Role::Subscriber => "Subscribers+",
            Role::Vip => "VIPs+",
            Role::Moderator => "Moderators+",
            Role::Broadcaster => "Streamer only",
        }
    }
}

/// Who defined the command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Owner {
    /// Created by the streamer in the widget.
    User,
    /// Registered by another plugin (its plugin name, e.g. `pato_obs`).
    Plugin(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    /// Replies in chat with a canned message. `{user}`, `{username}`,
    /// `{platform}` and `{<argument name>}` are substituted.
    Info { response: String },
    /// Signals the owning plugin; handled entirely by that plugin.
    Trigger,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Canonical name: lowercase, unique across every name and alias.
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub owner: Owner,
    pub kind: Kind,
    /// Argument template, see `parser`. Empty = takes no arguments.
    #[serde(default)]
    pub args: String,
    pub min_role: Role,
    #[serde(default)]
    pub global_cooldown_secs: u32,
    #[serde(default)]
    pub user_cooldown_secs: u32,
    /// Moderators and the broadcaster ignore (and don't start) cooldowns.
    #[serde(default = "yes")]
    pub bypass_cooldown: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl Command {
    /// Normalises (trim + lowercase names) and validates everything that can
    /// be checked without seeing the other commands.
    fn normalize(mut self) -> Result<Command, String> {
        self.name = normalize_name(&self.name)?;
        let mut aliases = Vec::new();
        for alias in &self.aliases {
            let alias = normalize_name(alias)?;
            if alias == self.name || aliases.contains(&alias) {
                return Err(format!("Duplicate name \"{alias}\""));
            }
            aliases.push(alias);
        }
        self.aliases = aliases;
        self.args = self.args.trim().to_string();
        let parts = parser::validate_template(&self.args)?;
        match (&self.owner, &mut self.kind) {
            (Owner::User, Kind::Trigger) => {
                return Err("Only a plugin can own a trigger command".into());
            }
            (Owner::Plugin(_), Kind::Info { .. }) => {
                return Err("Plugin commands must be triggers".into());
            }
            (Owner::Plugin(p), _) if p.is_empty() => return Err("Plugin owner is empty".into()),
            (_, Kind::Info { response }) => {
                *response = response.trim().to_string();
                if response.is_empty() {
                    return Err("Response cannot be empty".into());
                }
                if response.chars().count() > MAX_RESPONSE_LEN {
                    return Err(format!("Response is longer than {MAX_RESPONSE_LEN} characters"));
                }
                for part in &parts {
                    if RESERVED_VARS.contains(&part.name.as_str()) {
                        return Err(format!("Argument name \"{}\" is reserved", part.name));
                    }
                }
                for var in placeholders(response) {
                    let known = RESERVED_VARS.contains(&var) || parts.iter().any(|p| p.name == var);
                    if !known {
                        return Err(format!("Unknown placeholder {{{var}}} in response"));
                    }
                }
            }
            (Owner::Plugin(_), Kind::Trigger) => {}
        }
        if self.global_cooldown_secs > MAX_COOLDOWN_SECS || self.user_cooldown_secs > MAX_COOLDOWN_SECS {
            return Err("Cooldown cannot exceed 24 hours".into());
        }
        Ok(self)
    }

    /// Every name this command answers to, canonical first.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.name.as_str()).chain(self.aliases.iter().map(String::as_str))
    }
}

fn normalize_name(raw: &str) -> Result<String, String> {
    let name = raw.trim().to_lowercase();
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(format!("Name \"{name}\" is longer than {MAX_NAME_LEN} characters"));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(format!("Name \"{name}\" may only use letters, digits, - and _"));
    }
    Ok(name)
}

/// The identifiers inside `{...}` in `text`. Anything that isn't a plain
/// `{word}` is left alone by substitution and ignored here.
pub fn placeholders(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let inner = &after[..close];
                if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    found.push(inner);
                }
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    found
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// Master switch. While off, no chat line triggers any command; each
    /// command's own `enabled` flag is left alone, so switching back on
    /// restores exactly what was active before.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Command prefix per platform (`twitch` -> `!`). A platform with no
    /// entry never triggers commands.
    pub prefixes: HashMap<String, String>,
    pub commands: Vec<Command>,
}

impl Default for State {
    fn default() -> Self {
        State {
            enabled: true,
            prefixes: HashMap::from([("twitch".to_string(), "!".to_string())]),
            commands: Vec::new(),
        }
    }
}

impl State {
    /// The command answering to `name` (canonical or alias, any case).
    pub fn find(&self, name: &str) -> Option<&Command> {
        let name = name.to_lowercase();
        self.commands.iter().find(|c| c.names().any(|n| n == name))
    }

    /// Adds `command`, or replaces the existing command with the same
    /// canonical name. Fails (leaving state untouched) if it is invalid or
    /// any of its names collides with another command's name or alias.
    pub fn upsert(&mut self, command: Command) -> Result<(), String> {
        let command = command.normalize()?;
        for name in command.names() {
            if let Some(other) = self.commands.iter().find(|c| c.name != command.name && c.names().any(|n| n == name)) {
                return Err(format!("\"{name}\" is already used by command \"{}\"", other.name));
            }
        }
        match self.commands.iter_mut().find(|c| c.name == command.name) {
            Some(existing) => *existing = command,
            None => self.commands.push(command),
        }
        Ok(())
    }

    /// Like `upsert`, but refuses to overwrite an existing command of the
    /// same name - for "create new" where that would be a silent clobber.
    pub fn add(&mut self, command: Command) -> Result<(), String> {
        let name = command.name.trim().to_lowercase();
        if self.commands.iter().any(|c| c.name == name) {
            return Err(format!("A command named \"{name}\" already exists"));
        }
        self.upsert(command)
    }

    /// Replaces the command named `old` with `command`, which may have a new
    /// name. All-or-nothing: on any error nothing changes.
    pub fn replace(&mut self, old: &str, command: Command) -> Result<(), String> {
        let new_name = command.name.trim().to_lowercase();
        if new_name != old.to_lowercase() && self.commands.iter().any(|c| c.name == new_name) {
            return Err(format!("A command named \"{new_name}\" already exists"));
        }
        let mut next = self.clone();
        next.remove(old);
        next.upsert(command)?;
        *self = next;
        Ok(())
    }

    /// Removes the command with canonical name `name`; false if none.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.commands.len();
        let name = name.to_lowercase();
        self.commands.retain(|c| c.name != name);
        self.commands.len() != before
    }

    /// Sets a platform's prefix. Must be 1-8 visible characters, no spaces.
    pub fn set_prefix(&mut self, platform: &str, prefix: &str) -> Result<(), String> {
        let prefix = prefix.trim();
        if prefix.is_empty() || prefix.chars().count() > 8 || prefix.chars().any(char::is_whitespace) {
            return Err("Prefix must be 1-8 characters with no spaces".into());
        }
        self.prefixes.insert(platform.to_string(), prefix.to_string());
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn info(name: &str, response: &str) -> Command {
    Command {
        name: name.into(),
        aliases: vec![],
        owner: Owner::User,
        kind: Kind::Info { response: response.into() },
        args: String::new(),
        min_role: Role::Everyone,
        global_cooldown_secs: 0,
        user_cooldown_secs: 0,
        bypass_cooldown: true,
        enabled: true,
    }
}

#[cfg(test)]
pub(crate) fn trigger(name: &str, owner: &str) -> Command {
    Command {
        kind: Kind::Trigger,
        owner: Owner::Plugin(owner.into()),
        ..info(name, "unused")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_names_to_lowercase_and_trims() {
        let mut s = State::default();
        let mut c = info("  Socials ", "hi");
        c.aliases = vec!["LINKS".into()];
        s.upsert(c).unwrap();
        assert_eq!(s.commands[0].name, "socials");
        assert_eq!(s.commands[0].aliases, ["links"]);
        assert_eq!(s.find("LiNkS").unwrap().name, "socials");
    }

    #[test]
    fn rejects_bad_names() {
        let mut s = State::default();
        for bad in ["", "   ", "has space", "bang!", "ünï", &"x".repeat(33)] {
            assert!(s.upsert(info(bad, "hi")).is_err(), "{bad:?}");
        }
        assert!(s.commands.is_empty());
    }

    #[test]
    fn names_and_aliases_must_be_unique_across_commands() {
        let mut s = State::default();
        let mut a = info("a", "hi");
        a.aliases = vec!["shared".into()];
        s.upsert(a).unwrap();
        assert!(s.upsert(info("shared", "x")).unwrap_err().contains("already used by command \"a\""));
        let mut b = info("b", "x");
        b.aliases = vec!["A".into()];
        assert!(s.upsert(b).is_err());
        assert_eq!(s.commands.len(), 1);
    }

    #[test]
    fn rejects_duplicate_aliases_within_one_command() {
        let mut c = info("a", "hi");
        c.aliases = vec!["x".into(), "X".into()];
        assert!(State::default().upsert(c).is_err());
        let mut c = info("a", "hi");
        c.aliases = vec!["A".into()];
        assert!(State::default().upsert(c).is_err());
    }

    #[test]
    fn upsert_replaces_same_name_and_may_keep_its_own_aliases() {
        let mut s = State::default();
        let mut c = info("a", "one");
        c.aliases = vec!["x".into()];
        s.upsert(c.clone()).unwrap();
        c.kind = Kind::Info { response: "two".into() };
        s.upsert(c).unwrap();
        assert_eq!(s.commands.len(), 1);
        assert_eq!(s.commands[0].kind, Kind::Info { response: "two".into() });
    }

    #[test]
    fn owner_and_kind_must_agree() {
        let mut s = State::default();
        let mut bad = trigger("t", "pato_obs");
        bad.owner = Owner::User;
        assert!(s.upsert(bad).is_err());
        let mut bad = info("i", "hi");
        bad.owner = Owner::Plugin("pato_obs".into());
        assert!(s.upsert(bad).is_err());
        s.upsert(trigger("t", "pato_obs")).unwrap();
    }

    #[test]
    fn validates_arg_template_and_response_placeholders() {
        let mut s = State::default();
        let mut c = info("hug", "{user} hugs {target}!");
        c.args = "<target:string>".into();
        s.upsert(c).unwrap();

        let mut c = info("bad", "x");
        c.args = "<oops>".into();
        assert!(s.upsert(c).is_err());

        let mut c = info("typo", "hello {taget}");
        c.args = "<target:string>".into();
        assert!(s.upsert(c).unwrap_err().contains("{taget}"));

        let mut c = info("shadow", "x");
        c.args = "<user:string>".into();
        assert!(s.upsert(c).unwrap_err().contains("reserved"));
    }

    #[test]
    fn response_limits() {
        let mut s = State::default();
        assert!(s.upsert(info("a", "   ")).is_err());
        assert!(s.upsert(info("b", &"x".repeat(501))).is_err());
        assert!(s.upsert(info("c", &"x".repeat(500))).is_ok());
    }

    #[test]
    fn braces_that_are_not_placeholders_are_left_alone() {
        assert_eq!(placeholders("a {b} {c d} {} {e_1} {unclosed"), ["b", "e_1"]);
        assert!(State::default().upsert(info("a", "json: {\"x\": 1} and :{")).is_ok());
    }

    #[test]
    fn cooldown_cap() {
        let mut c = info("a", "hi");
        c.global_cooldown_secs = MAX_COOLDOWN_SECS + 1;
        assert!(State::default().upsert(c).is_err());
        let mut c = info("a", "hi");
        c.user_cooldown_secs = MAX_COOLDOWN_SECS;
        assert!(State::default().upsert(c).is_ok());
    }

    #[test]
    fn add_refuses_to_clobber_but_replace_may_rename() {
        let mut s = State::default();
        s.add(info("a", "one")).unwrap();
        assert!(s.add(info(" A ", "two")).unwrap_err().contains("already exists"));
        assert_eq!(s.commands[0].kind, Kind::Info { response: "one".into() });

        s.add(info("b", "x")).unwrap();
        s.replace("a", info("renamed", "one")).unwrap();
        assert_eq!(s.commands.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["b", "renamed"]);
        // Renaming onto another command fails and leaves everything alone.
        let before = s.clone();
        assert!(s.replace("renamed", info("b", "x")).is_err());
        assert_eq!(s, before);
        // Keeping the same name is fine.
        s.replace("b", info("b", "changed")).unwrap();
    }

    #[test]
    fn role_identifiers_round_trip_and_follow_the_ladder() {
        for r in Role::ALL {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
        assert_eq!(Role::parse("admin"), None);
        assert!(Role::ALL.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn remove_and_prefix() {
        let mut s = State::default();
        s.upsert(info("a", "hi")).unwrap();
        assert!(s.remove("A"));
        assert!(!s.remove("a"));
        assert!(s.set_prefix("discord", "/").is_ok());
        for bad in ["", " ", "a b", "123456789"] {
            assert!(s.set_prefix("discord", bad).is_err(), "{bad:?}");
        }
        assert_eq!(s.prefixes["discord"], "/");
    }

    #[test]
    fn state_round_trips_through_json_and_tolerates_missing_optional_fields() {
        let mut s = State::default();
        s.upsert(info("a", "hi")).unwrap();
        s.upsert(trigger("scene", "pato_obs")).unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<State>(&json).unwrap(), s);

        let minimal = r#"{"prefixes":{},"commands":[{"name":"a","owner":"user",
            "kind":{"type":"info","response":"hi"},"min_role":"everyone"}]}"#;
        let c = &serde_json::from_str::<State>(minimal).unwrap().commands[0];
        assert!(c.enabled && c.bypass_cooldown);
        assert_eq!((c.global_cooldown_secs, c.user_cooldown_secs), (0, 0));
    }
}
