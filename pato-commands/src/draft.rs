//! What the widget's forms hold, and how a submitted form becomes a change to
//! the state. Pure (no WIT types) so the conversions are unit-tested natively.
//!
//! Drafts keep the raw strings the user typed so a rejected save re-renders
//! exactly what they entered.

use crate::model::{Command, Kind, Owner, Role, State};

#[derive(Clone, Debug, PartialEq)]
pub struct CommandDraft {
    pub name: String,
    /// Comma- and/or space-separated.
    pub aliases: String,
    pub response: String,
    pub args: String,
    pub min_role: Role,
    pub global_cooldown: String,
    pub user_cooldown: String,
    pub bypass_cooldown: bool,
    pub enabled: bool,
}

impl Default for CommandDraft {
    fn default() -> Self {
        CommandDraft {
            name: String::new(),
            aliases: String::new(),
            response: String::new(),
            args: String::new(),
            min_role: Role::Everyone,
            global_cooldown: "0".into(),
            user_cooldown: "0".into(),
            bypass_cooldown: true,
            enabled: true,
        }
    }
}

impl CommandDraft {
    pub fn from_command(c: &Command) -> Self {
        CommandDraft {
            name: c.name.clone(),
            aliases: c.aliases.join(", "),
            response: match &c.kind {
                Kind::Info { response } => response.clone(),
                Kind::Trigger => String::new(),
            },
            args: c.args.clone(),
            min_role: c.min_role,
            global_cooldown: c.global_cooldown_secs.to_string(),
            user_cooldown: c.user_cooldown_secs.to_string(),
            bypass_cooldown: c.bypass_cooldown,
            enabled: c.enabled,
        }
    }

    /// Read the submitted form fields by name. Switches arrive as
    /// "true"/"false"; the role comes from the radio group.
    pub fn from_fields(fields: &[(String, String)]) -> Self {
        let get = |name: &str| fields.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str()).unwrap_or("");
        CommandDraft {
            name: get("name").to_string(),
            aliases: get("aliases").to_string(),
            response: get("response").to_string(),
            args: get("args").to_string(),
            min_role: Role::parse(get("min_role")).unwrap_or(Role::Everyone),
            global_cooldown: get("global_cooldown").to_string(),
            user_cooldown: get("user_cooldown").to_string(),
            bypass_cooldown: get("bypass_cooldown") == "true",
            enabled: get("enabled") == "true",
        }
    }

    /// Builds the command to save. Only parses what needs parsing here (the
    /// numbers, the alias list); everything else is validated by `State`.
    ///
    /// When `editing` is a plugin's command, its identity (name, owner, kind,
    /// argument template) is fixed: only aliases, permissions, cooldowns and
    /// the enabled switch come from the form.
    pub fn to_command(&self, editing: Option<&Command>) -> Result<Command, String> {
        let global = cooldown(&self.global_cooldown, "Global cooldown")?;
        let user = cooldown(&self.user_cooldown, "Per-user cooldown")?;
        let aliases = self
            .aliases
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|a| !a.is_empty())
            .map(str::to_string)
            .collect();
        let base = match editing {
            Some(c) if c.owner != Owner::User => c.clone(),
            _ => Command {
                name: self.name.clone(),
                aliases: vec![],
                owner: Owner::User,
                kind: Kind::Info { response: self.response.clone() },
                args: self.args.clone(),
                min_role: Role::Everyone,
                global_cooldown_secs: 0,
                user_cooldown_secs: 0,
                bypass_cooldown: true,
                enabled: true,
            },
        };
        Ok(Command {
            aliases,
            min_role: self.min_role,
            global_cooldown_secs: global,
            user_cooldown_secs: user,
            bypass_cooldown: self.bypass_cooldown,
            enabled: self.enabled,
            ..base
        })
    }
}

fn cooldown(raw: &str, what: &str) -> Result<u32, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(0);
    }
    raw.parse::<u32>()
        .map_err(|_| format!("{what} must be a whole number of seconds (or 0 for none)."))
}

/// Applies the prefix form to a copy of `state`. Fields are `prefix:<platform>`
/// for each existing platform (empty removes it, which stops commands on that
/// platform) plus `new_platform` / `new_prefix` to add one. All-or-nothing.
pub fn apply_prefixes(state: &State, fields: &[(String, String)]) -> Result<State, String> {
    let mut next = state.clone();
    for (key, value) in fields {
        let Some(platform) = key.strip_prefix("prefix:") else { continue };
        if value.trim().is_empty() {
            next.prefixes.remove(platform);
        } else {
            next.set_prefix(platform, value).map_err(|e| format!("{platform}: {e}"))?;
        }
    }
    let get = |name: &str| fields.iter().find(|(k, _)| k == name).map(|(_, v)| v.trim()).unwrap_or("");
    let (platform, prefix) = (get("new_platform").to_lowercase(), get("new_prefix"));
    match (platform.is_empty(), prefix.is_empty()) {
        (true, true) => {}
        (true, false) => return Err("Enter a platform name for the new prefix.".into()),
        (false, true) => return Err(format!("Enter a prefix for \"{platform}\".")),
        (false, false) => {
            if !platform.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err("Platform names may only use letters, digits, - and _.".into());
            }
            next.set_prefix(&platform, prefix).map_err(|e| format!("{platform}: {e}"))?;
        }
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{info, trigger};

    fn fields(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn reads_form_fields_by_name() {
        let d = CommandDraft::from_fields(&fields(&[
            ("name", "hug"),
            ("aliases", "h, hugs"),
            ("response", "hi"),
            ("args", "<who:string>"),
            ("min_role", "vip"),
            ("global_cooldown", "5"),
            ("user_cooldown", "10"),
            ("bypass_cooldown", "false"),
            ("enabled", "true"),
        ]));
        assert_eq!(d.name, "hug");
        assert_eq!(d.min_role, Role::Vip);
        assert!(!d.bypass_cooldown && d.enabled);
        // Missing/garbage role falls back to the safest default for a *new* command.
        assert_eq!(CommandDraft::from_fields(&fields(&[("min_role", "admin")])).min_role, Role::Everyone);
    }

    #[test]
    fn new_command_becomes_a_user_info_command() {
        let d = CommandDraft {
            name: "hug".into(),
            aliases: "h, hugs  squeeze".into(),
            response: "{user} hugs".into(),
            global_cooldown: " 5 ".into(),
            user_cooldown: "".into(),
            ..Default::default()
        };
        let c = d.to_command(None).unwrap();
        assert_eq!(c.owner, Owner::User);
        assert_eq!(c.kind, Kind::Info { response: "{user} hugs".into() });
        assert_eq!(c.aliases, ["h", "hugs", "squeeze"]);
        assert_eq!((c.global_cooldown_secs, c.user_cooldown_secs), (5, 0));
        let mut s = State::default();
        s.add(c).unwrap();
    }

    #[test]
    fn bad_cooldowns_are_rejected_with_a_specific_message() {
        for bad in ["-1", "1.5", "soon", "99999999999"] {
            let d = CommandDraft { global_cooldown: bad.into(), ..Default::default() };
            assert!(d.to_command(None).unwrap_err().starts_with("Global cooldown"), "{bad}");
            let d = CommandDraft { user_cooldown: bad.into(), ..Default::default() };
            assert!(d.to_command(None).unwrap_err().starts_with("Per-user cooldown"), "{bad}");
        }
    }

    #[test]
    fn editing_a_plugin_command_keeps_its_identity() {
        let mut original = trigger("scene", "pato_obs");
        original.args = "<name:string...>".into();
        let d = CommandDraft {
            // A hand-crafted submission can't change identity fields.
            name: "evil".into(),
            response: "gotcha".into(),
            args: "".into(),
            aliases: "sc".into(),
            min_role: Role::Moderator,
            enabled: false,
            ..Default::default()
        };
        let c = d.to_command(Some(&original)).unwrap();
        assert_eq!((c.name.as_str(), &c.owner, &c.kind), ("scene", &Owner::Plugin("pato_obs".into()), &Kind::Trigger));
        assert_eq!(c.args, "<name:string...>");
        assert_eq!(c.aliases, ["sc"]);
        assert_eq!(c.min_role, Role::Moderator);
        assert!(!c.enabled);
    }

    #[test]
    fn draft_round_trips_a_command() {
        let mut c = info("hug", "hi {who}");
        c.args = "<who:string>".into();
        c.aliases = vec!["h".into(), "hugs".into()];
        c.min_role = Role::Subscriber;
        c.global_cooldown_secs = 3;
        assert_eq!(CommandDraft::from_command(&c).to_command(Some(&c)).unwrap(), c);
    }

    // --- prefixes ---------------------------------------------------------

    #[test]
    fn prefix_form_edits_removes_and_adds() {
        let mut s = State::default();
        s.set_prefix("discord", "/").unwrap();
        let next = apply_prefixes(
            &s,
            &fields(&[("prefix:twitch", "?"), ("prefix:discord", " "), ("new_platform", "Kick"), ("new_prefix", "$")]),
        )
        .unwrap();
        assert_eq!(next.prefixes["twitch"], "?");
        assert!(!next.prefixes.contains_key("discord"));
        assert_eq!(next.prefixes["kick"], "$");
    }

    #[test]
    fn prefix_form_is_all_or_nothing_and_explains_itself() {
        let s = State::default();
        let err = |f: &[(&str, &str)]| apply_prefixes(&s, &fields(f)).unwrap_err();
        assert!(err(&[("prefix:twitch", "a b")]).starts_with("twitch:"));
        assert!(err(&[("prefix:twitch", "?"), ("new_platform", "x")]).contains("Enter a prefix"));
        assert!(err(&[("new_prefix", "!")]).contains("Enter a platform"));
        assert!(err(&[("new_platform", "a b"), ("new_prefix", "!")]).contains("Platform names"));
        // Blank new row is simply ignored.
        assert!(apply_prefixes(&s, &fields(&[("new_platform", ""), ("new_prefix", "")])).is_ok());
        assert_eq!(s, State::default());
    }
}
