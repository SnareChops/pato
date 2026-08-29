use std::sync::{OnceLock, RwLock};

wit_bindgen::generate!({
    world: "twitch",
    path: "wit",
    generate_all
});

use crate::exports::pato::plugin;
use pato::plugin::storage;
use pato::plugin::widgets;
mod helix;
mod http;

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
    pid: String,
    status: TwitchStatusWidget,
    helix: Option<helix::TwitchApi>,
    user: Option<helix::TwitchUser>,
}
impl plugin::init::Guest for PatoTwitch {
    // Initialize the plugin (THIS IS THE STARTING POINT OF THE PLUGIN)
    fn init(pid: String) -> Result<bool, String> {
        // Create new PatoTwitch plugin instance and save it statically to be accessible later
        PLUGIN
            .set(RwLock::new(PatoTwitch::init(pid)?))
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
                if plugin.status.widget.id == id {
                    plugin.status.clicked();
                }
            }
            // If 'action' event
            plugin::widget_events::Event::Action(action) => {
                if plugin.status.widget.id == id {
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
            // If not successful: Set status to "Connect" and return
            if !success {
                plugin.status.widget.label = "Connect".to_string();
                plugin.status.update();
                return Ok(());
            }
            // Get response URL from authentication
            if let Some(response) = response {
                // Parse response as URL
                let url = url::Url::parse(&response).map_err(|e| e.to_string())?;
                // Get fragment from URL
                if let Some(frag) = url.fragment() {
                    // Split fragment into key-value pairs and iterate
                    for pair in frag
                        .split('&')
                        .map(|pair| pair.split_once('='))
                        .filter(|x| x.is_some())
                        .find(|pair| pair.unwrap_or(("", "")).0 == "access_token")
                        .and_then(|pair| match pair {
                            Some((_, value)) => plugin.login(value.to_string()),
                        })
                    {
                        // Get key and value from pair
                        if let Some((key, value)) = pair {
                            // If key is access_token: Save token and update status
                            if key == "access_token" {
                                plugin.login(value.to_string());
                                return Ok(());
                            }
                        }
                    }
                } else {
                    return Ok(());
                }
            } else {
                plugin.status.widget.label = "Connect".to_string();
                plugin.status.update();
                return Ok(());
            }
        })
    }
}

impl PatoTwitch {
    // Initialize PatoTwitch plugin
    fn init(pid: String) -> Result<Self, String> {
        // Try to get stored auth token, create Helix API if found
        let helix =
            storage::get(&pid, "auth_token").and_then(|token| Some(helix::TwitchApi::new(token)));
        // Try to get user info if Helix API was created
        let user = helix.as_ref().and_then(|api| api.get_my_user().ok());
        // Create plugin instance
        let mut plugin = Self {
            pid: pid.clone(),
            status: TwitchStatusWidget::init(pid)?,
            helix: helix,
            user: user,
        };
        // Set correct status based on current state
        if let Some(user) = &mut plugin.user {
            plugin.status.widget.label = user.display_name.clone();
            plugin.status.widget.icon = user.profile_image_url.clone();
        } else {
            plugin.status.widget.label = "Connect".to_string();
            plugin.status.widget.icon = "".to_string();
        }
        plugin.status.update();
        Ok(plugin)
    }

    // Try to login with existing token or start new connection
    fn login(&mut self, token: String) -> Result<bool, String> {
        // Create new Helix API instance
        self.helix = Some(helix::TwitchApi::new(token));
        // Try to get user info
        if let Some(api) = &self.helix {
            let user = api.get_my_user()?;
            // Set status to show user info
            self.status.widget.label = user.display_name.clone();
            self.status.widget.icon = user.profile_image_url.clone();
            self.status.update();
            // Save user
            self.user = Some(user);
            Ok(true)
        } else {
            self.status.widget.label = "Connect".to_string();
            self.status.widget.icon = "".to_string();
            self.status.update();
            Ok(false)
        }
    }
}

struct TwitchStatusWidget {
    pid: String,
    widget: widgets::StatusWidget,
}
impl TwitchStatusWidget {
    fn init(pid: String) -> Result<Self, String> {
        let widget = Self {
            pid,
            widget: widgets::StatusWidget {
                id: "twitch_status".to_string(),
                icon: "".to_string(),
                label: "Connect".to_string(),
                tooltip: Some("Connect to Twitch".to_string()),
                actions: vec![],
            },
        };
        widget.update();
        Ok(widget)
    }

    fn update(&self) {
        widgets::update(
            &self.pid,
            &widgets::Widget::StatusWidget(self.widget.clone()),
        );
    }

    fn clicked(&mut self) {
        if self.widget.label == "Connect" {
            self.connect();
        }
    }

    fn action(&self, action: String) {}

    fn connect(&mut self) {
        let url = format!(
            "https://id.twitch.tv/oauth2/authorize?client_id={}&redirect_uri={}&response_type=token&scope={}",
            CLIENT_ID,
            urlencoding::encode(REDIRECT_URI),
            urlencoding::encode(&SCOPES.join(" ")),
        );
        pato::plugin::auth::open_auth_window(&self.pid, "Connect to Twitch", url.as_str());
        self.widget.label = "Connecting".to_string();
        self.update();
    }
}

export!(PatoTwitch);
