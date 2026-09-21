use std::sync::{OnceLock, RwLock};

wit_bindgen::generate!({
    world: "discord",
    path: "wit",
    generate_all
});

use crate::exports::pato::plugin;
use pato::plugin::storage;
mod discord_api;
mod http;
mod widgets;

// Register an application at https://discord.com/developers/applications,
// add an OAuth2 redirect of `https://snarechops.net` (matches the host's
// auth-window domain check), and drop its client id in here.
const CLIENT_ID: &str = "1551029669757853726";
const REDIRECT_URI: &str = "https://snarechops.net";
// `identify` gets username/avatar; `email` additionally returns the account
// email from `/users/@me`.
const SCOPES: [&str; 2] = ["identify", "email"];

static PLUGIN: OnceLock<RwLock<PatoDiscord>> = OnceLock::new();

fn with_plugin<F, R>(f: F) -> R
where
    F: FnOnce(&mut PatoDiscord) -> R,
{
    let plugin = PLUGIN.get().unwrap();
    let mut plugin = plugin.write().unwrap();
    f(&mut plugin)
}

struct PatoDiscord {
    status: widgets::DiscordStatusWidget,
    api: Option<discord_api::DiscordApi>,
    user: Option<discord_api::DiscordUser>,
}
impl plugin::init::Guest for PatoDiscord {
    // Initialize the plugin (THIS IS THE STARTING POINT OF THE PLUGIN)
    fn init() -> Result<bool, String> {
        // Create new PatoDiscord plugin instance and save it statically to be accessible later
        PLUGIN
            .set(RwLock::new(PatoDiscord::init()?))
            .map_err(|_| "Failed to set plugin")?;
        // Return success
        Ok(true)
    }
}

impl plugin::widget_events::Guest for PatoDiscord {
    // Pre-made status widget: every interaction is a named action, no
    // separate "clicked" event.
    fn on_status_event(widget_id: String, action: String) {
        with_plugin(|plugin| {
            if plugin.status.id != widget_id {
                return;
            }
            // "disconnect" is handled centrally here since it affects every
            // widget that depends on the Discord connection, not just the
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
    // No custom widgets yet.
    fn on_event(_event: plugin::widget_events::WidgetEvent) {}

    // Widget was (re)sized on the grid. The status widget is pre-made and
    // fixed-size; nothing to do.
    fn on_layout(_layout: plugin::widget_events::WidgetLayout) {}
}

impl plugin::auth_events::Guest for PatoDiscord {
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
            // (Discord's implicit grant returns `#access_token=...&token_type=Bearer&...`)
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

impl PatoDiscord {
    // Initialize PatoDiscord plugin
    fn init() -> Result<Self, String> {
        // Try to get stored auth token, create Discord API client if found
        let api = storage::get("auth_token").map(discord_api::DiscordApi::new);
        // Try to get user info if the API client was created
        let user = api.as_ref().and_then(|api| api.get_my_user().ok());
        // Create plugin instance
        let mut plugin = Self {
            status: widgets::DiscordStatusWidget::new(),
            api,
            user,
        };
        // Set correct status based on current state
        match &plugin.user {
            Some(user) => plugin.status.connected(user),
            None => plugin.status.disconnected(),
        }
        Ok(plugin)
    }

    // Validate the token against Discord and, on success, persist it and show the user
    fn login(&mut self, token: String) -> Result<(), String> {
        let api = discord_api::DiscordApi::new(token.clone());
        match api.get_my_user() {
            Ok(user) => {
                // Persist the validated token for future sessions
                storage::set("auth_token", &token);
                self.status.connected(&user);
                self.user = Some(user);
                self.api = Some(api);
                Ok(())
            }
            Err(e) => {
                self.logout();
                Err(e)
            }
        }
    }

    // Sign out of Discord: drop the stored token and reset every widget that
    // depends on the connection back to its disconnected state. Triggered
    // either by the status widget's "Disconnect" action or by a token that
    // turned out to be invalid.
    fn logout(&mut self) {
        storage::del("auth_token");
        self.api = None;
        self.user = None;
        self.status.disconnected();
    }
}

export!(PatoDiscord);
