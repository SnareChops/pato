use std::ops::{Deref, DerefMut};

use crate::helix::TwitchUser;
use crate::pato::plugin::{auth, widgets};
use crate::{CLIENT_ID, REDIRECT_URI, SCOPES};

// Newtype over the generated `StatusWidget`. Holds no extra state — it exists
// only to hang Twitch-specific behaviour off the widget. `Deref`/`DerefMut`
// make the inner fields (`label`, `icon`, `id`, ...) directly accessible.
pub struct TwitchStatusWidget(widgets::StatusWidget);

impl Deref for TwitchStatusWidget {
    type Target = widgets::StatusWidget;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for TwitchStatusWidget {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl TwitchStatusWidget {
    pub fn new() -> Self {
        let widget = Self(widgets::StatusWidget {
            id: "twitch_status".to_string(),
            icon: "".to_string(),
            label: "Connect".to_string(),
            tooltip: Some("Connect to Twitch".to_string()),
            actions: vec![],
        });
        widget.update();
        widget
    }

    pub fn update(&self) {
        widgets::update(&widgets::Widget::StatusWidget(self.0.clone()));
    }

    pub fn clicked(&mut self) {
        if self.label == "Connect" {
            self.connect();
        }
    }

    // Show the connected Twitch user
    pub fn connected(&mut self, user: &TwitchUser) {
        self.label = user.display_name.clone();
        self.icon = user.profile_image_url.clone();
        self.update();
    }

    // Reset the widget to the disconnected "Connect" state
    pub fn disconnected(&mut self) {
        self.label = "Connect".to_string();
        self.icon = "".to_string();
        self.update();
    }

    pub fn action(&self, action: String) {}

    fn connect(&mut self) {
        let scope = SCOPES.join(" ");
        let url = url::Url::parse_with_params(
            "https://id.twitch.tv/oauth2/authorize",
            &[
                ("client_id", CLIENT_ID),
                ("redirect_uri", REDIRECT_URI),
                ("response_type", "token"),
                ("scope", scope.as_str()),
            ],
        )
        .expect("valid twitch auth url");
        auth::open_auth_window("Connect to Twitch", url.as_str());
        self.label = "Connecting".to_string();
        self.update();
    }
}
