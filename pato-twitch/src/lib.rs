use std::sync::{OnceLock, RwLock};

wit_bindgen::generate!({
    world: "twitch",
    path: "wit",
    generate_all
});

use crate::exports::pato::plugin;
use pato::plugin::storage;
mod helix;
mod http;
mod irc;
mod widgets;

const CLIENT_ID: &str = "arr27bhvnowepylzv8qs2tgaqc66yb";
const REDIRECT_URI: &str = "https://snarechops.net";
// `chat:read` / `chat:edit` authorize the IRC login (`PASS oauth:<token>`)
// used by the chat widget to read and send messages.
const SCOPES: [&str; 3] = ["user:read:email", "chat:read", "chat:edit"];

static PLUGIN: OnceLock<RwLock<PatoTwitch>> = OnceLock::new();

fn with_plugin<F, R>(f: F) -> R
where
    F: FnOnce(&mut PatoTwitch) -> R,
{
    let plugin = PLUGIN.get().unwrap();
    let mut plugin = plugin.write().unwrap();
    f(&mut plugin)
}

struct PatoTwitch {
    status: widgets::TwitchStatusWidget,
    panel: widgets::TwitchPanel,
    chat: widgets::TwitchChatWidget,
    helix: Option<helix::TwitchApi>,
    user: Option<helix::TwitchUser>,
}
impl plugin::init::Guest for PatoTwitch {
    // Initialize the plugin (THIS IS THE STARTING POINT OF THE PLUGIN)
    fn init() -> Result<bool, String> {
        // Create new PatoTwitch plugin instance and save it statically to be accessible later
        PLUGIN
            .set(RwLock::new(PatoTwitch::init()?))
            .map_err(|_| "Failed to set plugin")?;
        // Return success
        Ok(true)
    }
}

impl plugin::widget_events::Guest for PatoTwitch {
    // Pre-made status widget: every interaction is a named action, no
    // separate "clicked" event.
    fn on_status_event(widget_id: String, action: String) {
        with_plugin(|plugin| {
            if plugin.status.id != widget_id {
                return;
            }
            // "disconnect" is handled centrally here since it affects every
            // widget that depends on the Twitch connection, not just the
            // status widget itself; everything else is the widget's own
            // business.
            if action == widgets::ACTION_DISCONNECT {
                plugin.logout();
            } else {
                plugin.status.action(&action);
            }
        });
    }

    // Custom widget: interactions come back keyed by the element's `key`.
    fn on_event(event: plugin::widget_events::WidgetEvent) {
        with_plugin(|plugin| {
            if event.node_key == widgets::HANDLER_REFRESH {
                plugin.refresh_panel();
                return;
            }
            match event.kind {
                plugin::widget_events::EventKind::Input
                | plugin::widget_events::EventKind::Change => {
                    if let plugin::widget_events::Payload::Text(text) = event.payload {
                        plugin.chat.on_input(&event.node_key, text);
                    }
                }
                plugin::widget_events::EventKind::Click
                | plugin::widget_events::EventKind::EnterKey => {
                    let login = plugin.user.as_ref().map(|u| u.login.as_str());
                    let token = plugin.helix.as_ref().map(|h| h.token());
                    plugin.chat.on_click(&event.node_key, login, token);
                }
                _ => {}
            }
        });
    }

    // Widget was (re)sized on the grid. Both custom widgets are responsive;
    // nothing to do.
    fn on_layout(_layout: plugin::widget_events::WidgetLayout) {}
}

impl plugin::websocket_events::Guest for PatoTwitch {
    // A chat line arrived on an open IRC connection.
    fn on_message(id: String, data: String) {
        with_plugin(|plugin| plugin.chat.on_message(&id, &data));
    }

    fn on_close(id: String, code: u16, reason: String) {
        with_plugin(|plugin| {
            plugin
                .chat
                .on_closed(&id, format!("Disconnected ({code}): {reason}"))
        });
    }

    fn on_error(id: String, message: String) {
        with_plugin(|plugin| {
            plugin
                .chat
                .on_closed(&id, format!("Connection error: {message}"))
        });
    }
}

impl plugin::auth_events::Guest for PatoTwitch {
    // On authentication complete (or failed)
    fn on_auth(success: bool, response: Option<String>) -> Result<(), String> {
        with_plugin(move |plugin| {
            // If not successful: reset the widget and return
            if !success {
                plugin.status.disconnected();
                return Ok(());
            }
            // Get response URL from authentication
            let Some(response) = response else {
                plugin.status.disconnected();
                return Ok(());
            };
            // Parse response as URL
            let url = url::Url::parse(&response).map_err(|e| e.to_string())?;
            // Pull `access_token` out of the URL fragment
            // (Twitch implicit grant returns `#access_token=...&scope=...&token_type=bearer`)
            let token = url.fragment().and_then(|frag| {
                frag.split('&')
                    .filter_map(|pair| pair.split_once('='))
                    .find(|(key, _)| *key == "access_token")
                    .map(|(_, value)| value.to_string())
            });
            // If a token was found: login, otherwise reset the widget to "Connect"
            match token {
                Some(token) => {
                    plugin.login(token)?;
                }
                None => {
                    plugin.status.disconnected();
                }
            }
            Ok(())
        })
    }
}

impl PatoTwitch {
    // Initialize PatoTwitch plugin
    fn init() -> Result<Self, String> {
        // Try to get stored auth token, create Helix API if found
        let helix = storage::get("auth_token").map(helix::TwitchApi::new);
        // Try to get user info if Helix API was created
        let user = helix.as_ref().and_then(|api| api.get_my_user().ok());
        // Create plugin instance
        let mut plugin = Self {
            status: widgets::TwitchStatusWidget::new(),
            panel: widgets::TwitchPanel::new(),
            chat: widgets::TwitchChatWidget::new(),
            helix,
            user,
        };
        // Set correct status based on current state
        match &plugin.user {
            Some(user) => plugin.status.connected(user),
            None => plugin.status.disconnected(),
        }
        plugin.panel.set_user(plugin.user.as_ref());
        Ok(plugin)
    }

    // Validate the token against Helix and, on success, persist it and show the user
    fn login(&mut self, token: String) -> Result<(), String> {
        let api = helix::TwitchApi::new(token.clone());
        match api.get_my_user() {
            Ok(user) => {
                // Persist the validated token for future sessions
                storage::set("auth_token", &token);
                self.status.connected(&user);
                self.user = Some(user);
                self.helix = Some(api);
                self.panel.set_user(self.user.as_ref());
                Ok(())
            }
            Err(e) => {
                self.logout();
                Err(e)
            }
        }
    }

    // Sign out of Twitch: drop the stored token and reset every widget that
    // depends on the connection back to its disconnected state. Triggered
    // either by the status widget's "Disconnect" action or by a token that
    // turned out to be invalid.
    fn logout(&mut self) {
        storage::del("auth_token");
        self.helix = None;
        self.user = None;
        self.status.disconnected();
        self.panel.set_user(None);
        self.chat.force_disconnect();
    }

    // Re-fetch the current user from Helix and push it to the panel.
    fn refresh_panel(&mut self) {
        self.user = self.helix.as_ref().and_then(|api| api.get_my_user().ok());
        self.panel.set_user(self.user.as_ref());
    }
}

export!(PatoTwitch);
