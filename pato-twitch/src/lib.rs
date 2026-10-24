use std::sync::{OnceLock, RwLock};

wit_bindgen::generate!({
    world: "twitch",
    path: "wit",
    generate_all
});

use crate::exports::pato::pato_twitch::chat;
use crate::exports::pato::plugin;
use pato::plugin::storage;
mod commands;
mod helix;
mod http;
mod irc;
mod widgets;

const CLIENT_ID: &str = "arr27bhvnowepylzv8qs2tgaqc66yb";
const REDIRECT_URI: &str = "https://snarechops.net";
// `chat:read` / `chat:edit` authorize the IRC login (`PASS oauth:<token>`)
// used by the chat widget to read and send messages.
// `channel:manage:redemptions` lets the redeems widget list, create, edit and
// delete the channel's custom channel point rewards.
const SCOPES: [&str; 4] = [
    "user:read:email",
    "chat:read",
    "chat:edit",
    "channel:manage:redemptions",
];

const PRIMARY_CHANNEL_KEY: &str = "primary_channel";

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
    chat: widgets::TwitchChatWidget,
    redeems: widgets::TwitchRedeemsWidget,
    helix: Option<helix::TwitchApi>,
    user: Option<helix::TwitchUser>,
    // The primary channel when it isn't the signed-in account's own (e.g. a
    // streamer running Pato from a separate bot account). `None` follows the
    // signed-in account. Persisted under `PRIMARY_CHANNEL_KEY`.
    primary_override: Option<String>,
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
            } else if action == widgets::ACTION_PRIMARY_CHANNEL {
                plugin.open_primary_channel_prompt();
            } else {
                plugin.status.action(&action);
            }
        });
    }

    // Custom widget: interactions come back keyed by the element's `key`.
    fn on_event(event: plugin::widget_events::WidgetEvent) {
        with_plugin(|plugin| {
            if event.widget_id == widgets::primary_channel::MODAL_PRIMARY_CHANNEL {
                plugin.on_primary_channel_event(&event);
                return;
            }
            // The redeems widget and its modal own everything under those ids.
            if event.widget_id == widgets::REDEEMS_WIDGET_ID || event.widget_id == widgets::MODAL_FORM {
                plugin
                    .redeems
                    .on_event(&event, plugin.helix.as_ref(), plugin.user.as_ref());
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
                    plugin.chat.on_click(&event.node_key);
                }
                _ => {}
            }
        });
    }

    // Widget was (re)sized on the grid. The custom widgets are responsive;
    // nothing to do.
    fn on_layout(_layout: plugin::widget_events::WidgetLayout) {}

    // The user dismissed a modal without answering it.
    fn on_modal_closed(id: String) {
        with_plugin(|plugin| plugin.redeems.on_modal_closed(&id));
    }

    // The user answered a confirm prompt.
    fn on_confirm(id: String, confirmed: bool) {
        with_plugin(|plugin| {
            plugin
                .redeems
                .on_confirm(&id, confirmed, plugin.helix.as_ref(), plugin.user.as_ref())
        });
    }
}

// Public API other plugins import (`pato:pato-twitch/chat`) - see
// `pato-twitch/wit/api.wit`.
impl chat::Guest for PatoTwitch {
    fn send_message(text: String) -> bool {
        with_plugin(|plugin| plugin.chat.send_message(&text))
    }

    fn channel() -> Option<String> {
        with_plugin(|plugin| plugin.chat.channel())
    }
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
        let primary_override = storage::get(PRIMARY_CHANNEL_KEY);
        // Create plugin instance
        let mut plugin = Self {
            status: widgets::TwitchStatusWidget::new(),
            chat: widgets::TwitchChatWidget::new(),
            redeems: widgets::TwitchRedeemsWidget::new(),
            helix,
            user,
            primary_override,
        };
        plugin.apply_connection();
        Ok(plugin)
    }

    // Validate the token against Helix and, on success, persist it and show the user
    fn login(&mut self, token: String) -> Result<(), String> {
        let api = helix::TwitchApi::new(token.clone());
        match api.get_my_user() {
            Ok(user) => {
                // Persist the validated token for future sessions
                storage::set("auth_token", &token);
                self.user = Some(user);
                self.helix = Some(api);
                self.apply_connection();
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
        // The override belongs to the account that set it; the next sign-in
        // starts from its own channel again.
        storage::del(PRIMARY_CHANNEL_KEY);
        self.primary_override = None;
        self.helix = None;
        self.user = None;
        self.status.disconnected();
        self.chat.force_disconnect();
        self.redeems.set_connection(None, None, None);
    }

    // The channel chat, chat commands and redeems are tied to: the override
    // if one is set, otherwise the signed-in account's own channel.
    fn primary_channel(&self) -> Option<String> {
        let user = self.user.as_ref()?;
        Some(
            self.primary_override
                .clone()
                .unwrap_or_else(|| user.login.to_lowercase()),
        )
    }

    // Twitch only lets a channel's own account manage its channel point
    // rewards, so with a bot account signed in against someone else's channel
    // the redeems widget has to be blanked out and say why.
    fn redeems_conflict(&self) -> Option<String> {
        let user = self.user.as_ref()?;
        let primary = self.primary_channel()?;
        if primary == user.login.to_lowercase() {
            return None;
        }
        Some(format!(
            "Channel point redeems can only be managed by the channel's own account. \
             You are signed in as {} but the primary channel is #{primary}. Sign in \
             as {primary}, or set the primary channel back to your account.",
            user.display_name
        ))
    }

    // Push the current account + primary channel out to everything that
    // depends on them: the status widget, the one chat connection (which also
    // feeds commands) and the redeems widget.
    fn apply_connection(&mut self) {
        let (Some(user), Some(primary)) = (self.user.clone(), self.primary_channel()) else {
            self.status.disconnected();
            self.chat.force_disconnect();
            self.redeems.set_connection(None, None, None);
            return;
        };
        self.status.connected(&user, &primary);
        if let Some(helix) = &self.helix {
            self.chat.connect(&primary, &user.login, helix.token());
        }
        let conflict = self.redeems_conflict();
        self.redeems
            .set_connection(self.helix.as_ref(), self.user.as_ref(), conflict);
    }

    fn open_primary_channel_prompt(&self) {
        if let (Some(user), Some(primary)) = (&self.user, self.primary_channel()) {
            widgets::primary_channel::open(&primary, &user.login.to_lowercase(), None);
        }
    }

    fn on_primary_channel_event(&mut self, event: &plugin::widget_events::WidgetEvent) {
        use widgets::primary_channel::Outcome;
        let Some(account) = self.user.as_ref().map(|u| u.login.to_lowercase()) else {
            return;
        };
        let new = match widgets::primary_channel::on_event(event, &account) {
            Some(Outcome::Set(name)) => Some(name),
            Some(Outcome::Reset) => Some(account.clone()),
            _ => return,
        };
        let Some(name) = new else { return };
        if Some(&name) == self.primary_channel().as_ref() {
            return;
        }
        if name == account {
            storage::del(PRIMARY_CHANNEL_KEY);
            self.primary_override = None;
        } else {
            storage::set(PRIMARY_CHANNEL_KEY, &name);
            self.primary_override = Some(name);
        }
        self.apply_connection();
    }
}

export!(PatoTwitch);
