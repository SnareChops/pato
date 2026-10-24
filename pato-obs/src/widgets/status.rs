//! Status widget: shows the OBS connection state and, disconnected, offers
//! the action that opens the connect/settings modal (`crate::modal`).

use crate::pato::plugin::widgets;

const ID: &str = "obs_status";
pub const ACTION_CONNECT: &str = "connect";
pub const ACTION_SETTINGS: &str = "settings";
pub const ACTION_DISCONNECT: &str = "disconnect";

pub struct ObsStatusWidget(widgets::StatusWidget);

impl ObsStatusWidget {
    pub fn new() -> Self {
        let widget = Self(widgets::StatusWidget {
            id: ID.to_string(),
            icon: String::new(),
            label: "Connect".to_string(),
            tooltip: Some("OBS".to_string()),
            actions: vec![action(ACTION_CONNECT, "Connect", "Set up the OBS connection")],
        });
        widget.push();
        widget
    }

    fn push(&self) {
        widgets::update(&widgets::Widget::StatusWidget(self.0.clone()));
    }

    pub fn connecting(&mut self) {
        self.0.label = "Connecting\u{2026}".to_string();
        self.0.actions = vec![]; // nothing to pick mid-handshake
        self.push();
    }

    pub fn connected(&mut self, host: &str) {
        self.0.label = "Connected".to_string();
        self.0.tooltip = Some(format!("OBS \u{2014} {host}"));
        self.0.actions = vec![
            action(ACTION_SETTINGS, "Settings", "Change the OBS connection"),
            action(ACTION_DISCONNECT, "Disconnect", "Disconnect from OBS"),
        ];
        self.push();
    }

    pub fn disconnected(&mut self, error: Option<&str>) {
        self.0.label = "Connect".to_string();
        self.0.tooltip = Some(error.map(|e| format!("OBS: {e}")).unwrap_or_else(|| "OBS".to_string()));
        self.0.actions = vec![action(ACTION_CONNECT, "Connect", "Set up the OBS connection")];
        self.push();
    }
}

fn action(id: &str, text: &str, description: &str) -> widgets::Action {
    widgets::Action {
        id: id.to_string(),
        text: text.to_string(),
        description: Some(description.to_string()),
    }
}
