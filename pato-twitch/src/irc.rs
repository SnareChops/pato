//! Minimal Twitch IRC-over-WebSocket client.
//!
//! `pato:plugin/websocket` is a generic, vendor-agnostic secure relay - the
//! core knows nothing about Twitch. Every byte of the IRC wire protocol
//! (login handshake, PRIVMSG framing, PING/PONG keepalive) is this plugin's
//! own responsibility. No tags capability is requested, so this is
//! deliberately basic: no display-name casing, colors, badges or emotes -
//! just the IRC nick (lowercase login) and message text.

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

pub enum IrcEvent {
    Message { username: String, text: String },
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
    for line in data.split("\r\n").filter(|l| !l.is_empty()) {
        if let Some(server) = line.strip_prefix("PING ") {
            send_line(conn_id, &format!("PONG {server}"));
            continue;
        }
        if let Some(reason) = parse_notice_to_unregistered(line) {
            events.push(IrcEvent::AuthFailed(reason.to_string()));
            continue;
        }
        if let Some(event) = parse_privmsg(line) {
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

/// `:<nick>!<user>@<host>.tmi.twitch.tv PRIVMSG #<channel> :<message text>`
fn parse_privmsg(line: &str) -> Option<IrcEvent> {
    let rest = line.strip_prefix(':')?;
    let (prefix, rest) = rest.split_once(' ')?;
    let username = prefix.split('!').next()?.to_string();
    let rest = rest.strip_prefix("PRIVMSG #")?;
    let (_channel, text) = rest.split_once(" :")?;
    Some(IrcEvent::Message {
        username,
        text: text.to_string(),
    })
}
