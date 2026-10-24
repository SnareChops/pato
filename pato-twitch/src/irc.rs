//! Minimal Twitch IRC-over-WebSocket client.
//!
//! `pato:plugin/websocket` is a generic, vendor-agnostic secure relay - the
//! core knows nothing about Twitch. Every byte of the IRC wire protocol
//! (login handshake, PRIVMSG framing, PING/PONG keepalive) is this plugin's
//! own responsibility. Only the `twitch.tv/tags` capability is requested:
//! enough for display name, user id and badges (which drive the command
//! system's role checks), but not emotes or colors.

use std::collections::HashMap;

use crate::pato::plugin::websocket;

const IRC_WS_URL: &str = "wss://irc-ws.chat.twitch.tv:443";

pub struct IrcConnection {
    pub id: String,
    pub channel: String,
    // Twitch's IRC doesn't echo a client's own PRIVMSGs back (no
    // echo-message capability support), so callers use this to show a sent
    // message locally themselves.
    pub self_login: String,
}

/// A chatter's standing in the channel, ordered lowest to highest so roles
/// can be compared with `>=`. Derived from the message's badges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Everyone,
    Subscriber,
    Vip,
    Moderator,
    Broadcaster,
}

pub enum IrcEvent {
    Message {
        // Lowercase IRC nick (login).
        username: String,
        // Display-cased name; falls back to `username` when Twitch omits it.
        display_name: String,
        user_id: String,
        role: Role,
        text: String,
    },
    // `USERSTATE`: Twitch tells us our own badges on join and after each
    // message we send. Twitch doesn't echo our own PRIVMSGs back, so this is
    // the only way to know our role for messages we type ourselves.
    SelfState { role: Role, display_name: String },
    // Twitch phrases this several ways ("Login unsuccessful", "Login
    // authentication failed", "Improperly formatted auth", ...) - carries
    // the server's own text rather than guessing at a fixed list.
    AuthFailed(String),
}

/// Opens the IRC connection and fires off the login/join handshake. Twitch
/// doesn't ack a handshake line-by-line, so this returns as soon as the
/// socket itself is open; a bad token surfaces later as a NOTICE line
/// handled by `handle_frame` (see `IrcEvent::AuthFailed`).
pub fn connect(login: &str, token: &str, channel: &str) -> Result<IrcConnection, String> {
    let id = websocket::connect(IRC_WS_URL)?;
    let channel = channel.trim().trim_start_matches('#').to_lowercase();
    let login = login.to_lowercase();
    // Must precede the handshake so tags are on from the first message.
    send_line(&id, "CAP REQ :twitch.tv/tags twitch.tv/commands");
    send_line(&id, &format!("PASS oauth:{token}"));
    send_line(&id, &format!("NICK {login}"));
    send_line(&id, &format!("JOIN #{channel}"));
    Ok(IrcConnection {
        id,
        channel,
        self_login: login,
    })
}

pub fn send_message(conn: &IrcConnection, text: &str) {
    send_line(&conn.id, &format!("PRIVMSG #{} :{text}", conn.channel));
}

pub fn disconnect(conn: &IrcConnection) {
    websocket::close(&conn.id);
}

fn send_line(id: &str, line: &str) {
    websocket::send(id, &format!("{line}\r\n"));
}

/// Handles one WebSocket text frame, which may bundle several `\r\n`
/// terminated IRC lines. Answers PINGs itself (protocol-level keepalive,
/// not something the caller needs to know about); returns the events the
/// caller should act on.
pub fn handle_frame(conn_id: &str, data: &str) -> Vec<IrcEvent> {
    let mut events = Vec::new();
    for raw in data.split("\r\n").filter(|l| !l.is_empty()) {
        let (tags, line) = split_tags(raw);
        if let Some(server) = line.strip_prefix("PING ") {
            send_line(conn_id, &format!("PONG {server}"));
            continue;
        }
        if let Some(reason) = parse_notice_to_unregistered(line) {
            events.push(IrcEvent::AuthFailed(reason.to_string()));
            continue;
        }
        if let Some(event) = parse_privmsg(&tags, line).or_else(|| parse_userstate(&tags, line)) {
            events.push(event);
        }
    }
    events
}

/// `:tmi.twitch.tv NOTICE * :<reason>` - IRC targets a NOTICE at `*` (rather
/// than a nick or channel) specifically for the not-yet-registered
/// connection, which for Twitch means the login/CAP handshake failed. Twitch
/// phrases the reason several ways ("Login unsuccessful", "Login
/// authentication failed", "Improperly formatted auth", ...), so this
/// matches on the `NOTICE *` target rather than a fixed list of messages.
fn parse_notice_to_unregistered(line: &str) -> Option<&str> {
    let rest = line.strip_prefix(':')?;
    let (_source, rest) = rest.split_once(' ')?;
    rest.strip_prefix("NOTICE * :")
}

/// With the tags capability a line may start with `@key=value;key=value `.
/// Returns the unescaped tags and the remainder of the line.
fn split_tags(line: &str) -> (HashMap<String, String>, &str) {
    let Some(rest) = line.strip_prefix('@') else {
        return (HashMap::new(), line);
    };
    let (raw, rest) = rest.split_once(' ').unwrap_or((rest, ""));
    let tags = raw
        .split(';')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (!key.is_empty()).then(|| (key.to_string(), unescape_tag_value(value)))
        })
        .collect();
    (tags, rest)
}

/// IRCv3 tag value escaping: `\:` `;`, `\s` space, `\\` backslash, `\r`, `\n`.
fn unescape_tag_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(':') => out.push(';'),
            Some('s') => out.push(' '),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Highest role the `badges` tag (`broadcaster/1,subscriber/12`) or the `mod`
/// tag grants. `founder` is a subscriber badge variant.
fn role_from_tags(tags: &HashMap<String, String>) -> Role {
    let badges = tags.get("badges").map(String::as_str).unwrap_or("");
    let mut role = if tags.get("mod").map(String::as_str) == Some("1") {
        Role::Moderator
    } else {
        Role::Everyone
    };
    for badge in badges.split(',') {
        let name = badge.split('/').next().unwrap_or("");
        let granted = match name {
            "broadcaster" => Role::Broadcaster,
            "moderator" => Role::Moderator,
            "vip" => Role::Vip,
            "subscriber" | "founder" => Role::Subscriber,
            _ => Role::Everyone,
        };
        role = role.max(granted);
    }
    role
}

/// `:<nick>!<user>@<host>.tmi.twitch.tv PRIVMSG #<channel> :<message text>`
fn parse_privmsg(tags: &HashMap<String, String>, line: &str) -> Option<IrcEvent> {
    let rest = line.strip_prefix(':')?;
    let (prefix, rest) = rest.split_once(' ')?;
    let username = prefix.split('!').next()?.to_string();
    let rest = rest.strip_prefix("PRIVMSG #")?;
    let (_channel, text) = rest.split_once(" :")?;
    let display_name = tags
        .get("display-name")
        .filter(|n| !n.is_empty())
        .cloned()
        .unwrap_or_else(|| username.clone());
    Some(IrcEvent::Message {
        user_id: tags.get("user-id").cloned().unwrap_or_default(),
        role: role_from_tags(tags),
        display_name,
        username,
        text: text.to_string(),
    })
}

/// `:tmi.twitch.tv USERSTATE #<channel>`
fn parse_userstate(tags: &HashMap<String, String>, line: &str) -> Option<IrcEvent> {
    let rest = line.strip_prefix(':')?;
    let (_source, rest) = rest.split_once(' ')?;
    rest.strip_prefix("USERSTATE #")?;
    Some(IrcEvent::SelfState {
        role: role_from_tags(tags),
        display_name: tags.get("display-name").cloned().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(line: &str) -> (String, String, String, Role, String) {
        let (tags, line) = split_tags(line);
        match parse_privmsg(&tags, line) {
            Some(IrcEvent::Message { username, display_name, user_id, role, text }) => {
                (username, display_name, user_id, role, text)
            }
            _ => panic!("not a message"),
        }
    }

    #[test]
    fn parses_tagged_privmsg() {
        let line = "@badges=moderator/1,subscriber/6;display-name=Snare\\sChops;user-id=42 :snarechops!snarechops@snarechops.tmi.twitch.tv PRIVMSG #chan :!clip 10";
        let (user, display, id, role, text) = message(line);
        assert_eq!(user, "snarechops");
        assert_eq!(display, "Snare Chops");
        assert_eq!(id, "42");
        assert_eq!(role, Role::Moderator);
        assert_eq!(text, "!clip 10");
    }

    #[test]
    fn untagged_privmsg_still_parses_with_defaults() {
        let (user, display, id, role, text) =
            message(":a!a@a.tmi.twitch.tv PRIVMSG #chan :hello");
        assert_eq!((user.as_str(), display.as_str(), id.as_str()), ("a", "a", ""));
        assert_eq!(role, Role::Everyone);
        assert_eq!(text, "hello");
    }

    #[test]
    fn highest_badge_wins() {
        let tags = |s: &str| split_tags(&format!("@{s} :x")).0;
        assert_eq!(role_from_tags(&tags("badges=subscriber/1,vip/1")), Role::Vip);
        assert_eq!(role_from_tags(&tags("badges=vip/1,broadcaster/1")), Role::Broadcaster);
        assert_eq!(role_from_tags(&tags("badges=founder/0")), Role::Subscriber);
        assert_eq!(role_from_tags(&tags("badges=;mod=1")), Role::Moderator);
        assert_eq!(role_from_tags(&tags("badges=premium/1")), Role::Everyone);
    }

    #[test]
    fn tagged_notice_to_unregistered_is_still_recognised() {
        let (_, line) = split_tags("@msg-id=x :tmi.twitch.tv NOTICE * :Login unsuccessful");
        assert_eq!(parse_notice_to_unregistered(line), Some("Login unsuccessful"));
    }

    #[test]
    fn userstate_reports_our_own_role() {
        let (tags, line) = split_tags(
            "@badges=vip/1;display-name=Snare :tmi.twitch.tv USERSTATE #chan",
        );
        match parse_userstate(&tags, line) {
            Some(IrcEvent::SelfState { role, display_name }) => {
                assert_eq!(role, Role::Vip);
                assert_eq!(display_name, "Snare");
            }
            _ => panic!("not a userstate"),
        }
        let (tags, line) = split_tags(":a!a@a.tmi.twitch.tv PRIVMSG #chan :USERSTATE #x");
        assert!(parse_userstate(&tags, line).is_none());
    }

    #[test]
    fn unescapes_tag_values() {
        assert_eq!(unescape_tag_value("a\\sb\\:c\\\\d"), "a b;c\\d");
    }
}
