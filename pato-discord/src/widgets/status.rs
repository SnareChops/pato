use std::ops::{Deref, DerefMut};

use crate::discord_api::DiscordUser;
use crate::pato::plugin::{auth, widgets};
use crate::{CLIENT_ID, REDIRECT_URI, SCOPES};

// Newtype over the generated `StatusWidget`. Holds no extra state — it exists
// only to hang Discord-specific behaviour off the widget. `Deref`/`DerefMut`
// make the inner fields (`label`, `icon`, `id`, ...) directly accessible.
pub struct DiscordStatusWidget(widgets::StatusWidget);

impl Deref for DiscordStatusWidget {
    type Target = widgets::StatusWidget;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for DiscordStatusWidget {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

const ACTION_CONNECT: &str = "connect";
// Handled by `PatoDiscord::logout` (`lib.rs`), not here, since it affects
// every widget that depends on the connection - not just this one.
pub const ACTION_DISCONNECT: &str = "disconnect";

impl DiscordStatusWidget {
    pub fn new() -> Self {
        let widget = Self(widgets::StatusWidget {
            id: "discord_status".to_string(),
            icon: "".to_string(),
            label: "Connect".to_string(),
            tooltip: Some("Discord".to_string()),
            actions: vec![action(ACTION_CONNECT, "Connect", "Connect to Discord")],
        });
        widget.update();
        widget
    }

    pub fn update(&self) {
        widgets::update(&widgets::Widget::StatusWidget(self.0.clone()));
    }

    // Every interaction is a named action - there is no separate "clicked"
    // event. `disconnect` is handled by the caller (`PatoDiscord::logout`)
    // since it affects more than this widget.
    pub fn action(&mut self, action: &str) {
        if action == ACTION_CONNECT {
            self.connect();
        }
    }

    // Show the connected Discord user, with a "Disconnect" action in the
    // widget's dropdown.
    pub fn connected(&mut self, user: &DiscordUser) {
        self.label = user.display_name().to_string();
        self.icon = user.avatar_url();
        self.actions = vec![action(ACTION_DISCONNECT, "Disconnect", "Sign out of Discord")];
        self.update();
    }

    // Reset the widget to the disconnected "Connect" state
    pub fn disconnected(&mut self) {
        self.label = "Connect".to_string();
        self.icon = "".to_string();
        self.actions = vec![action(ACTION_CONNECT, "Connect", "Connect to Discord")];
        self.update();
    }

    fn connect(&mut self) {
        let scope = SCOPES.join(" ");
        let url = url::Url::parse_with_params(
            "https://discord.com/oauth2/authorize",
            &[
                ("client_id", CLIENT_ID),
                ("redirect_uri", REDIRECT_URI),
                ("response_type", "token"),
                ("scope", scope.as_str()),
            ],
        )
        .expect("valid discord auth url");
        auth::open_auth_window("Connect to Discord", url.as_str());
        self.label = "Connecting".to_string();
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
