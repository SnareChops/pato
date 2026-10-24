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

const ACTION_CONNECT: &str = "connect";
// Handled by `PatoTwitch::logout` (`lib.rs`), not here, since it affects
// every widget that depends on the connection - not just this one.
pub const ACTION_DISCONNECT: &str = "disconnect";
// Also handled in `lib.rs`: it needs the plugin's settings and the modal.
pub const ACTION_PRIMARY_CHANNEL: &str = "primary-channel";

// The brand icon ships in this plugin's own `assets/` (see `assets.rs`), so the
// widget identifies its platform at a glance.
const ICON: &str = "pato-asset:icon.svg";

impl TwitchStatusWidget {
    pub fn new() -> Self {
        let widget = Self(widgets::StatusWidget {
            id: "twitch_status".to_string(),
            icon: ICON.to_string(),
            label: String::new(),
            state: widgets::LinkState::Disconnected,
            tooltip: Some("Twitch".to_string()),
            actions: vec![action(ACTION_CONNECT, "Connect", "Connect to Twitch")],
        });
        widget.update();
        widget
    }

    pub fn update(&self) {
        widgets::update(&widgets::Widget::StatusWidget(self.0.clone()));
    }

    // Every interaction is a named action - there is no separate "clicked"
    // event. `disconnect` is handled by the caller (`PatoTwitch::logout`)
    // since it affects more than this widget.
    pub fn action(&mut self, action: &str) {
        if action == ACTION_CONNECT {
            self.connect();
        }
    }

    // Show the connected Twitch user, with the primary channel setting and a
    // "Disconnect" action in the widget's dropdown. The tooltip names the
    // primary channel, since it can differ from the signed-in account.
    pub fn connected(&mut self, user: &TwitchUser, primary_channel: &str) {
        self.label = user.display_name.clone();
        self.state = widgets::LinkState::Connected;
        self.icon = ICON.to_string();
        self.tooltip = Some(format!("Twitch - primary channel #{primary_channel}"));
        self.actions = vec![
            action(
                ACTION_PRIMARY_CHANNEL,
                "Primary channel...",
                "Choose the channel chat, commands and redeems use",
            ),
            action(ACTION_DISCONNECT, "Disconnect", "Sign out of Twitch"),
        ];
        self.update();
    }

    // Reset the widget to the disconnected state; the "Connect" action stays
    // in the dropdown.
    pub fn disconnected(&mut self) {
        self.label = String::new();
        self.state = widgets::LinkState::Disconnected;
        self.icon = ICON.to_string();
        self.tooltip = Some("Twitch".to_string());
        self.actions = vec![action(ACTION_CONNECT, "Connect", "Connect to Twitch")];
        self.update();
    }

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
        self.label = String::new();
        self.state = widgets::LinkState::Connecting;
        // Nothing to pick mid-flow.
        self.actions = vec![];
        self.update();
    }
}

fn action(id: &str, text: &str, description: &str) -> widgets::Action {
    widgets::Action {
        id: id.to_string(),
        text: text.to_string(),
        description: Some(description.to_string()),
    }
}
