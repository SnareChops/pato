//! Status widget: shows the VTube Studio connection state and offers
//! connect / disconnect.

use crate::pato::plugin::widgets;

pub const ID: &str = "vts_status";
pub const ACTION_CONNECT: &str = "connect";
pub const ACTION_DISCONNECT: &str = "disconnect";

pub struct VtsStatusWidget(widgets::StatusWidget);

impl VtsStatusWidget {
    pub fn new() -> Self {
        let widget = Self(widgets::StatusWidget {
            id: ID.to_string(),
            icon: String::new(),
            label: "Connect".to_string(),
            tooltip: Some("VTube Studio".to_string()),
            actions: vec![connect_action()],
        });
        widget.push();
        widget
    }

    fn push(&self) {
        widgets::update(&widgets::Widget::StatusWidget(self.0.clone()));
    }

    pub fn connecting(&mut self) {
        self.0.label = "Connecting\u{2026}".to_string();
        self.0.tooltip = Some("VTube Studio".to_string());
        self.0.actions = vec![]; // nothing to pick mid-handshake
        self.push();
    }

    /// The token request is out; the user has to click Allow in VTS.
    pub fn awaiting_approval(&mut self) {
        self.0.label = "Approve in VTS".to_string();
        self.0.tooltip = Some("VTube Studio \u{2014} approve the plugin popup in VTube Studio".to_string());
        self.0.actions = vec![disconnect_action("Cancel")];
        self.push();
    }

    pub fn connected(&mut self) {
        self.0.label = "Connected".to_string();
        self.0.tooltip = Some("VTube Studio".to_string());
        self.0.actions = vec![disconnect_action("Disconnect")];
        self.push();
    }

    pub fn disconnected(&mut self, error: Option<&str>) {
        self.0.label = "Connect".to_string();
        self.0.tooltip = Some(error.map(|e| format!("VTube Studio: {e}")).unwrap_or_else(|| "VTube Studio".to_string()));
        self.0.actions = vec![connect_action()];
        self.push();
    }
}

fn connect_action() -> widgets::Action {
    widgets::Action {
        id: ACTION_CONNECT.to_string(),
        text: "Connect".to_string(),
        description: Some("Connect to VTube Studio".to_string()),
    }
}

fn disconnect_action(text: &str) -> widgets::Action {
    widgets::Action {
        id: ACTION_DISCONNECT.to_string(),
        text: text.to_string(),
        description: Some("Disconnect from VTube Studio".to_string()),
    }
}
