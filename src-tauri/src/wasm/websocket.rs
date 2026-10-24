//! Generic, vendor-agnostic WebSocket relay for plugins.
//!
//! The core owns the real socket - same trust-boundary role as
//! `wasi:http`'s outgoing-handler for plain HTTP requests - but knows
//! nothing about what is sent over it. `connect` blocks (from the guest's
//! perspective) until the handshake with the peer completes or fails; every
//! byte after that is opaque text frames relayed verbatim in both
//! directions. Protocol semantics (handshake sequence, framing, parsing)
//! are entirely the calling plugin's responsibility - see `pato-twitch`'s
//! own Twitch IRC client for an example.
//!
//! `wss://` is required except for `ws://` to a loopback host (see
//! `is_permitted`) - the one carve-out for local control-plane protocols
//! (e.g. obs-websocket) that have no TLS story of their own.

use std::sync::LazyLock;

use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use wasmtime::component::{HasSelf, Val};

use crate::wasm;

mod plugin {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "websocket-host",
        imports: { default: async },
    });
}
use plugin::pato::plugin::websocket;

const WEBSOCKET_EVENTS: &str = "pato:plugin/websocket-events";

/// One open connection, owned by the plugin that opened it. `outgoing` feeds
/// the write half running on `run_connection`; dropping it (on `close`)
/// unblocks that task's `rx.recv()` with `None`, which closes the socket.
struct Connection {
    pid: String,
    outgoing: mpsc::UnboundedSender<Message>,
}

static CONNECTIONS: LazyLock<DashMap<String, Connection>> = LazyLock::new(DashMap::new);

/// obs-websocket (and anything else that only ever runs as a same-machine
/// control plane) has no TLS story of its own - it's plain `ws://` or
/// nothing. Plaintext is only acceptable when it can't leave the machine, so
/// `ws://` is allowed exclusively to a loopback host; everything else still
/// requires `wss://`. Resolving `localhost` isn't attempted - only the
/// literal loopback forms are accepted, so a plugin can't launder a remote
/// `ws://` connection through a hosts-file trick.
fn is_permitted(url: &str) -> bool {
    if url.starts_with("wss://") {
        return true;
    }
    let Some(parsed) = url::Url::parse(url).ok().filter(|u| u.scheme() == "ws") else {
        return false;
    };
    match parsed.host() {
        Some(url::Host::Domain(h)) => h.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

impl websocket::Host for wasm::HostData {
    async fn connect(&mut self, url: String) -> Result<String, String> {
        if !is_permitted(&url) {
            return Err(
                "only wss:// URLs, or ws:// to localhost/127.0.0.1/::1, are permitted".to_string(),
            );
        }
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| e.to_string())?;

        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel();
        CONNECTIONS.insert(
            id.clone(),
            Connection {
                pid: self.pid.clone(),
                outgoing: tx,
            },
        );

        // Spawn onto the *ambient* runtime - each plugin drives its own
        // dedicated single-threaded tokio runtime (see `run_plugin_thread`),
        // and `connect_async` above just registered this socket's reactor
        // state with that specific runtime. `tauri::async_runtime::spawn`
        // would hand it to Tauri's separate main runtime instead, which
        // silently breaks the socket's readiness notifications (surfaces as
        // a spurious reset a few seconds in). `tokio::spawn` picks up the
        // runtime that's currently polling this future, keeping it in place.
        tokio::spawn(run_connection(id.clone(), self.pid.clone(), ws, rx));
        Ok(id)
    }

    async fn send(&mut self, id: String, data: String) -> bool {
        match CONNECTIONS.get(&id) {
            Some(conn) if conn.pid == self.pid => {
                conn.outgoing.send(Message::Text(data.into())).is_ok()
            }
            _ => false,
        }
    }

    async fn close(&mut self, id: String) {
        CONNECTIONS.remove_if(&id, |_, conn| conn.pid == self.pid);
    }
}

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

/// Owns the socket's read and write halves for one connection's lifetime:
/// forwards outgoing frames from `rx` to the peer and dispatches incoming
/// frames to the owning plugin, until either side closes or errors.
async fn run_connection(id: String, pid: String, ws: WsStream, mut rx: mpsc::UnboundedReceiver<Message>) {
    let (mut write, mut read) = ws.split();

    loop {
        tokio::select! {
            outgoing = rx.recv() => match outgoing {
                Some(msg) => {
                    if write.send(msg).await.is_err() {
                        break;
                    }
                }
                // `close` dropped the sender.
                None => {
                    let _ = write.close().await;
                    break;
                }
            },
            incoming = read.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    dispatch(&pid, "on-message", vec![Val::String(id.clone()), Val::String(text.to_string())]).await;
                }
                Some(Ok(Message::Close(frame))) => {
                    let (code, reason) = frame
                        .map(|f| (u16::from(f.code), f.reason.to_string()))
                        .unwrap_or((1000, String::new()));
                    dispatch(&pid, "on-close", vec![Val::String(id.clone()), Val::U16(code), Val::String(reason)]).await;
                    break;
                }
                Some(Ok(_)) => {} // binary / ping / pong - out of scope for this relay
                Some(Err(e)) => {
                    dispatch(&pid, "on-error", vec![Val::String(id.clone()), Val::String(e.to_string())]).await;
                    break;
                }
                None => {
                    dispatch(&pid, "on-close", vec![Val::String(id.clone()), Val::U16(1006), Val::String("connection closed".to_string())]).await;
                    break;
                }
            },
        }
    }
    CONNECTIONS.remove(&id);
}

async fn dispatch(pid: &str, func: &str, params: Vec<Val>) {
    if let Err(e) = wasm::plugin_call(pid, WEBSOCKET_EVENTS, func, params, vec![]).await {
        eprintln!("{WEBSOCKET_EVENTS}|{func} dispatch to {pid} failed: {e}");
    }
}

pub fn init() -> Result<(), String> {
    println!("Initializing Websocket module...");
    wasm::link(|linker| websocket::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wss_always_permitted() {
        assert!(is_permitted("wss://irc-ws.chat.twitch.tv:443"));
        assert!(is_permitted("wss://example.com"));
    }

    #[test]
    fn ws_permitted_only_to_loopback() {
        assert!(is_permitted("ws://127.0.0.1:4455"));
        assert!(is_permitted("ws://localhost:4455"));
        assert!(is_permitted("ws://LOCALHOST:4455"));
        assert!(is_permitted("ws://[::1]:4455"));
        assert!(!is_permitted("ws://192.168.1.50:4455"));
        assert!(!is_permitted("ws://example.com"));
        assert!(!is_permitted("ws://127.0.0.1.evil.com"));
    }

    #[test]
    fn rejects_other_schemes_and_garbage() {
        assert!(!is_permitted("http://127.0.0.1"));
        assert!(!is_permitted("not a url"));
        assert!(!is_permitted(""));
    }
}
