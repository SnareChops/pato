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
    panel: widgets::TwitchPanel,
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
    // Pre-made status widget: click / action
    fn on_status_event(widget_id: String, event: plugin::widget_events::StatusEvent) {
        with_plugin(|plugin| {
            if plugin.status.id != widget_id {
                return;
            }
            match event {
                plugin::widget_events::StatusEvent::Clicked => plugin.status.clicked(),
                plugin::widget_events::StatusEvent::Action(action) => plugin.status.action(action),
            }
        });
    }

    // Custom widget: interactions come back keyed by the element's `key`.
    fn on_event(event: plugin::widget_events::WidgetEvent) {
        with_plugin(|plugin| {
            if event.node_key == widgets::HANDLER_REFRESH {
                plugin.refresh_panel();
            }
        });
    }

    // Widget was (re)sized on the grid. The panel is responsive; nothing to do.
    fn on_layout(_layout: plugin::widget_events::WidgetLayout) {}
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
                // Clear any stale state and reset the widget
                storage::del("auth_token");
                self.helix = None;
                self.user = None;
                self.status.disconnected();
                self.panel.set_user(None);
                Err(e)
            }
        }
    }

    // Re-fetch the current user from Helix and push it to the panel.
    fn refresh_panel(&mut self) {
        self.user = self.helix.as_ref().and_then(|api| api.get_my_user().ok());
        self.panel.set_user(self.user.as_ref());
    }
}

export!(PatoTwitch);
