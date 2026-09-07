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
mod widgets;

const CLIENT_ID: &str = "arr27bhvnowepylzv8qs2tgaqc66yb";
const REDIRECT_URI: &str = "https://snarechops.net";
const SCOPES: [&str; 1] = ["user:read:email"];

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
    // Handle widget events
    fn on_event(event: plugin::widget_events::Event, id: String) {
        with_plugin(|plugin| match event {
            // If 'clicked' event
            plugin::widget_events::Event::Clicked => {
                if plugin.status.id == id {
                    plugin.status.clicked();
                }
            }
            // If 'action' event
            plugin::widget_events::Event::Action(action) => {
                if plugin.status.id == id {
                    plugin.status.action(action);
                }
            }
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
            helix,
            user,
        };
        // Set correct status based on current state
        match &plugin.user {
            Some(user) => plugin.status.connected(user),
            None => plugin.status.disconnected(),
        }
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
                Ok(())
            }
            Err(e) => {
                // Clear any stale state and reset the widget
                storage::del("auth_token");
                self.helix = None;
                self.user = None;
                self.status.disconnected();
                Err(e)
            }
        }
    }
}

export!(PatoTwitch);
