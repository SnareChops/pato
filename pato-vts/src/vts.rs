//! VTube Studio connection state machine: open the socket, authenticate with
//! the stored token (or request a new one, which the user approves inside
//! VTS), and track the result.

use crate::pato::plugin::{storage, websocket};
use crate::protocol::{self, Envelope};

const KEY_TOKEN: &str = "vts_token";

/// What `Vts::on_message` hands back to the caller; handshake traffic is
/// consumed internally and returns `None`.
pub enum Incoming {
    /// The handshake just finished.
    Connected,
    /// A response or event received while connected.
    Message(Envelope),
}

enum State {
    Disconnected { error: Option<String> },
    /// Socket open, waiting for the user to approve the plugin in VTS.
    AwaitingApproval { conn_id: String },
    /// Socket open, `AuthenticationRequest` sent.
    Authenticating { conn_id: String },
    Connected { conn_id: String },
}

pub struct Vts {
    state: State,
}

impl Vts {
    pub fn new() -> Self {
        Vts { state: State::Disconnected { error: None } }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Connected { .. })
    }
    pub fn is_awaiting_approval(&self) -> bool {
        matches!(self.state, State::AwaitingApproval { .. })
    }
    pub fn is_connecting(&self) -> bool {
        matches!(self.state, State::Authenticating { .. })
    }
    pub fn last_error(&self) -> Option<&str> {
        match &self.state {
            State::Disconnected { error } => error.as_deref(),
            _ => None,
        }
    }

    /// `host`/`port` must be a loopback address per the core's `ws://`
    /// policy - see `src-tauri/src/wasm/websocket.rs::is_permitted`.
    pub fn connect(&mut self, host: &str, port: u16) {
        self.disconnect();
        let conn_id = match websocket::connect(&format!("ws://{host}:{port}")) {
            Ok(id) => id,
            Err(e) => {
                self.state = State::Disconnected { error: Some(e) };
                return;
            }
        };
        match storage::get(KEY_TOKEN) {
            Some(token) if !token.is_empty() => self.authenticate(conn_id, &token),
            _ => self.request_token(conn_id),
        }
    }

    pub fn disconnect(&mut self) {
        if let Some(id) = self.conn_id() {
            websocket::close(id);
        }
        self.state = State::Disconnected { error: None };
    }

    fn conn_id(&self) -> Option<&str> {
        match &self.state {
            State::AwaitingApproval { conn_id }
            | State::Authenticating { conn_id }
            | State::Connected { conn_id } => Some(conn_id),
            State::Disconnected { .. } => None,
        }
    }

    fn authenticate(&mut self, conn_id: String, token: &str) {
        websocket::send(&conn_id, &protocol::authentication_request(token));
        self.state = State::Authenticating { conn_id };
    }

    fn request_token(&mut self, conn_id: String) {
        websocket::send(&conn_id, &protocol::token_request());
        self.state = State::AwaitingApproval { conn_id };
    }

    /// Drop the connection and report `error`.
    fn fail(&mut self, error: String) {
        if let Some(id) = self.conn_id() {
            websocket::close(id);
        }
        self.state = State::Disconnected { error: Some(error) };
    }

    // --- transport events (from `PatoVts::on_message` / `on_close` / `on_error`) ---

    pub fn on_close(&mut self, conn_id: &str, reason: String) {
        if self.conn_id() == Some(conn_id) {
            self.state = State::Disconnected { error: Some(reason) };
        }
    }

    /// Sends a request on the open connection. `false` if not connected.
    pub fn send(&self, message: &str) -> bool {
        match &self.state {
            State::Connected { conn_id } => websocket::send(conn_id, message),
            _ => false,
        }
    }

    pub fn on_message(&mut self, conn_id: &str, data: &str) -> Option<Incoming> {
        if self.conn_id() != Some(conn_id) {
            return None; // a stale connection's tail messages after we moved on
        }
        let env = serde_json::from_str::<Envelope>(data).ok()?;
        let conn_id = conn_id.to_string();
        let message_type = env.message_type.clone();
        match message_type.as_str() {
            protocol::TYPE_TOKEN_RESPONSE => {
                let res = serde_json::from_value::<protocol::TokenResponse>(env.data).ok()?;
                storage::set(KEY_TOKEN, &res.token);
                self.authenticate(conn_id, &res.token);
                None
            }
            protocol::TYPE_AUTH_RESPONSE => {
                let res = serde_json::from_value::<protocol::AuthResponse>(env.data).ok()?;
                if res.authenticated {
                    self.state = State::Connected { conn_id };
                    Some(Incoming::Connected)
                } else {
                    // Stored token was revoked or is stale: ask for a fresh one.
                    storage::set(KEY_TOKEN, "");
                    self.request_token(conn_id);
                    None
                }
            }
            protocol::TYPE_API_ERROR => {
                let err = serde_json::from_value::<protocol::ApiError>(env.data).ok()?;
                if self.is_connected() {
                    return None; // post-handshake errors aren't fatal to the connection
                }
                let reason = if err.error_id == protocol::ERROR_TOKEN_REQUEST_DENIED {
                    "Plugin access was denied in VTube Studio".to_string()
                } else {
                    format!("VTube Studio error {}: {}", err.error_id, err.message)
                };
                self.fail(reason);
                None
            }
            _ if self.is_connected() => Some(Incoming::Message(env)),
            _ => None,
        }
    }
}
