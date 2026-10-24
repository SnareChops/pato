use std::sync::{OnceLock, RwLock};

wit_bindgen::generate!({
    world: "vts",
    path: "wit",
    generate_all
});

use crate::exports::pato::plugin;
use pato::plugin::timers;

mod modal;
mod model;
mod positions;
mod protocol;
mod vts;
mod widgets;

// VTube Studio's default API address; both are configurable in VTS.
const HOST: &str = "localhost";
const PORT: u16 = 8001;

// Set once a token has been stored: later runs reconnect without the user
// having to click Connect (unauthenticated, VTS would pop an approval prompt).
const KEY_TOKEN: &str = "vts_token";

// How often to re-read model position and expression state while connected,
// so changes made outside Pato (VTS UI, hotkeys, other plugins) show up.
const POLL_INTERVAL_MS: u32 = 1000;

// Events that keep the model widget current without polling.
const SUBSCRIBED_EVENTS: [&str; 3] = ["ModelLoadedEvent", "ModelConfigChangedEvent", "TrackingStatusChangedEvent"];

static PLUGIN: OnceLock<RwLock<PatoVts>> = OnceLock::new();

fn with_plugin<F, R>(f: F) -> R
where
    F: FnOnce(&mut PatoVts) -> R,
{
    let plugin = PLUGIN.get().unwrap();
    let mut plugin = plugin.write().unwrap();
    f(&mut plugin)
}

struct PatoVts {
    status: widgets::VtsStatusWidget,
    model_widget: widgets::VtsModelWidget,
    vts: vts::Vts,
    model: model::Model,
    positions: positions::Positions,
    /// The position being saved (captured when "+" was clicked) or edited.
    editing: Option<Editing>,
    /// "+" was clicked and we're waiting for a fresh `CurrentModelResponse`
    /// so the saved position is where the model is now, not up to a poll ago.
    capture_pending: bool,
    poll_timer: Option<String>,
}

enum Editing {
    New(model::Position),
    Existing(u32),
}

impl plugin::init::Guest for PatoVts {
    fn init() -> Result<bool, String> {
        PLUGIN
            .set(RwLock::new(PatoVts::init()))
            .map_err(|_| "Failed to set plugin")?;
        Ok(true)
    }
}

impl plugin::widget_events::Guest for PatoVts {
    fn on_status_event(widget_id: String, action: String) {
        with_plugin(|plugin| {
            if widget_id != widgets::status::ID {
                return;
            }
            match action.as_str() {
                widgets::status::ACTION_CONNECT => plugin.connect(),
                widgets::status::ACTION_DISCONNECT => {
                    plugin.vts.disconnect();
                    plugin.sync_status();
                }
                _ => {}
            }
        });
    }

    fn on_event(event: plugin::widget_events::WidgetEvent) {
        with_plugin(|plugin| {
            use plugin::widget_events::{EventKind, Payload};
            if event.widget_id == modal::MODAL_ID {
                match (&event.kind, &event.payload) {
                    (EventKind::Click, _) if event.node_key == modal::HANDLER_CANCEL => {
                        modal::close();
                        plugin.editing = None;
                    }
                    (EventKind::Click, _) if event.node_key == modal::HANDLER_DELETE => plugin.ask_delete(),
                    (EventKind::Submit, Payload::Fields(fields)) if event.node_key == modal::HANDLER_SUBMIT => {
                        plugin.submit_position(modal::Submitted::from_fields(fields));
                    }
                    _ => {}
                }
            } else if event.widget_id == widgets::model::WIDGET_ID {
                match event.kind {
                    EventKind::Click => plugin.on_model_click(&event.node_key),
                    EventKind::ContextMenu => plugin.on_model_context_menu(&event.node_key),
                    _ => {}
                }
            }
        });
    }
    fn on_layout(_layout: plugin::widget_events::WidgetLayout) {}
    fn on_modal_closed(id: String) {
        with_plugin(|plugin| {
            if id == modal::MODAL_ID {
                plugin.editing = None;
            }
        });
    }
    fn on_confirm(id: String, confirmed: bool) {
        with_plugin(|plugin| {
            if id == modal::CONFIRM_DELETE_ID && confirmed {
                plugin.delete_editing();
            }
        });
    }
}

impl plugin::timers_events::Guest for PatoVts {
    fn on_tick(id: String) {
        with_plugin(|plugin| {
            if plugin.poll_timer.as_deref() != Some(id.as_str()) {
                return;
            }
            plugin.vts.send(&protocol::current_model());
            plugin.vts.send(&protocol::expression_state());
        });
    }
}

impl plugin::websocket_events::Guest for PatoVts {
    fn on_message(id: String, data: String) {
        with_plugin(|plugin| {
            match plugin.vts.on_message(&id, &data) {
                Some(vts::Incoming::Connected) => plugin.on_connected(),
                Some(vts::Incoming::Message(env)) => plugin.on_vts_message(env),
                None => {}
            }
            plugin.sync_status();
        });
    }
    fn on_close(id: String, code: u16, reason: String) {
        with_plugin(|plugin| {
            plugin.vts.on_close(&id, format!("Disconnected ({code}): {reason}"));
            plugin.sync_status();
        });
    }
    fn on_error(id: String, message: String) {
        with_plugin(|plugin| {
            plugin.vts.on_close(&id, format!("Connection error: {message}"));
            plugin.sync_status();
        });
    }
}

impl PatoVts {
    fn init() -> Self {
        let mut plugin = PatoVts {
            status: widgets::VtsStatusWidget::new(),
            model_widget: widgets::VtsModelWidget::new(),
            vts: vts::Vts::new(),
            model: model::Model::default(),
            positions: positions::Positions::default(),
            editing: None,
            capture_pending: false,
            poll_timer: None,
        };
        plugin.render_model();
        if pato::plugin::storage::get(KEY_TOKEN).is_some_and(|t| !t.is_empty()) {
            plugin.connect();
        }
        plugin
    }

    fn connect(&mut self) {
        self.vts.connect(HOST, PORT);
        self.sync_status();
    }

    fn render_model(&self) {
        self.model_widget.render(self.vts.is_connected(), &self.model, &self.positions.presets);
    }

    /// Handshake finished: subscribe to events and load the initial state.
    fn on_connected(&mut self) {
        for event in SUBSCRIBED_EVENTS {
            self.vts.send(&protocol::subscribe(event));
        }
        self.fetch_all();
    }

    fn fetch_all(&self) {
        self.vts.send(&protocol::current_model());
        self.vts.send(&protocol::expression_state());
        self.vts.send(&protocol::hotkeys_in_current_model());
        self.vts.send(&protocol::face_found());
    }

    fn on_vts_message(&mut self, env: protocol::Envelope) {
        let is_current_model = env.message_type == "CurrentModelResponse";
        let applied = self.model.apply(&env.message_type, env.data);
        let positions_changed = self.sync_positions();
        if applied.refresh {
            self.fetch_all();
        }
        if applied.changed || positions_changed {
            self.render_model();
        }
        if is_current_model && self.capture_pending {
            self.capture_pending = false;
            self.begin_save();
        }
    }

    /// Load the saved positions of whichever model is now loaded. Returns
    /// whether the list shown changed.
    fn sync_positions(&mut self) -> bool {
        if self.positions.model_id() == self.model.id() {
            return false;
        }
        self.positions = positions::Positions::load(self.model.id());
        self.editing = None;
        true
    }

    /// Open the form for a position at where the model is right now.
    fn begin_save(&mut self) {
        let Some(at) = self.model.position.clone() else {
            return;
        };
        if modal::open(false, "", &modal::default_duration(), None) {
            self.editing = Some(Editing::New(at));
        }
    }

    fn on_model_context_menu(&mut self, handler: &str) {
        let Some(id) = handler.strip_prefix(widgets::model::PREFIX_POSITION_MENU).and_then(|i| i.parse().ok()) else {
            return;
        };
        let Some(preset) = self.positions.get(id) else {
            return;
        };
        if modal::open(true, &preset.name, &preset.duration.to_string(), None) {
            self.editing = Some(Editing::Existing(id));
        }
    }

    fn submit_position(&mut self, form: modal::Submitted) {
        let editing = matches!(self.editing, Some(Editing::Existing(_)));
        let (name, duration) = match form.validate() {
            Ok(v) => v,
            Err(e) => {
                modal::open(editing, &form.name, &form.duration, Some(&e));
                return;
            }
        };
        match self.editing.take() {
            Some(Editing::New(at)) => self.positions.add(name, &at, positions::clamp_duration(duration)),
            Some(Editing::Existing(id)) => self.positions.edit(id, name, positions::clamp_duration(duration)),
            None => return,
        }
        modal::close();
        self.render_model();
    }

    fn ask_delete(&self) {
        if let Some(Editing::Existing(id)) = self.editing {
            if let Some(preset) = self.positions.get(id) {
                modal::confirm_delete(&preset.name);
            }
        }
    }

    fn delete_editing(&mut self) {
        if let Some(Editing::Existing(id)) = self.editing.take() {
            self.positions.remove(id);
            modal::close();
            self.render_model();
        }
    }

    fn on_model_click(&mut self, handler: &str) {
        if let Some(file) = handler.strip_prefix(widgets::model::PREFIX_EXPRESSION) {
            let Some(active) = self.model.expressions.iter().find(|e| e.file == file).map(|e| e.active) else {
                return;
            };
            if self.vts.send(&protocol::expression_activation(file, !active)) {
                self.model.set_expression_active(file, !active);
                self.render_model();
            }
        } else if handler == widgets::model::HANDLER_ADD_POSITION {
            // Re-read the position first: the polled one can be a second old.
            self.capture_pending = self.vts.send(&protocol::current_model());
        } else if let Some(preset) = handler
            .strip_prefix(widgets::model::PREFIX_POSITION)
            .and_then(|id| id.parse().ok())
            .and_then(|id| self.positions.get(id))
        {
            self.vts.send(&protocol::move_model(preset.duration, preset.x, preset.y, preset.rotation, preset.size));
        } else if let Some(id) = handler.strip_prefix(widgets::model::PREFIX_HOTKEY) {
            self.vts.send(&protocol::hotkey_trigger(id));
            // A hotkey may flip expressions; don't wait for the next poll.
            self.vts.send(&protocol::expression_state());
        }
    }

    /// Run the poll timer exactly while connected.
    fn sync_timer(&mut self) {
        match (self.vts.is_connected(), self.poll_timer.is_some()) {
            (true, false) => match timers::set_interval(POLL_INTERVAL_MS) {
                Ok(id) => self.poll_timer = Some(id),
                Err(e) => log_warn(&format!("could not start poll timer: {e}")),
            },
            (false, true) => {
                if let Some(id) = self.poll_timer.take() {
                    timers::cancel(&id);
                }
            }
            _ => {}
        }
    }

    /// Reflect `self.vts`'s current connection state onto the status widget,
    /// the poll timer and the model widget.
    fn sync_status(&mut self) {
        let was_connected_view = self.poll_timer.is_some();
        self.sync_timer();
        if !self.vts.is_connected() && was_connected_view {
            self.model.reset();
            self.sync_positions();
            self.capture_pending = false;
            self.render_model();
        } else if self.vts.is_connected() && !was_connected_view {
            self.render_model();
        }
        if self.vts.is_connected() {
            self.status.connected();
        } else if self.vts.is_awaiting_approval() {
            self.status.awaiting_approval();
        } else if self.vts.is_connecting() {
            self.status.connecting();
        } else {
            self.status.disconnected(self.vts.last_error());
        }
    }
}

export!(PatoVts);

fn log_warn(message: &str) {
    eprintln!("pato-vts: {message}");
}
