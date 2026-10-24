use std::sync::{OnceLock, RwLock};

wit_bindgen::generate!({
    world: "obs",
    path: "wit",
    generate_all
});

use crate::exports::pato::pato_obs::scenes;
use crate::exports::pato::plugin;
use pato::plugin::storage;
mod modal;
mod obs;
mod protocol;
mod widgets;

const KEY_HOST: &str = "obs_host";
const KEY_PORT: &str = "obs_port";
const KEY_PASSWORD: &str = "obs_password";

static PLUGIN: OnceLock<RwLock<PatoObs>> = OnceLock::new();

fn with_plugin<F, R>(f: F) -> R
where
    F: FnOnce(&mut PatoObs) -> R,
{
    let plugin = PLUGIN.get().unwrap();
    let mut plugin = plugin.write().unwrap();
    f(&mut plugin)
}

struct PatoObs {
    status: widgets::ObsStatusWidget,
    obs: obs::Obs,
}

impl plugin::init::Guest for PatoObs {
    fn init() -> Result<bool, String> {
        PLUGIN
            .set(RwLock::new(PatoObs::init()?))
            .map_err(|_| "Failed to set plugin")?;
        Ok(true)
    }
}

impl plugin::widget_events::Guest for PatoObs {
    // No status-widget-style pre-made widgets besides this one; the
    // scenes/sources API (`obs.rs`) has no widget yet to click on.
    fn on_status_event(widget_id: String, action: String) {
        with_plugin(|plugin| {
            if widget_id != "obs_status" {
                return;
            }
            match action.as_str() {
                widgets::status::ACTION_CONNECT | widgets::status::ACTION_SETTINGS => {
                    let (host, port, password) = plugin.saved_connection();
                    let host = if host.is_empty() { modal::DEFAULT_HOST.to_string() } else { host };
                    modal::open(&host, &port, &password, None);
                }
                widgets::status::ACTION_DISCONNECT => {
                    plugin.obs.disconnect();
                    plugin.status.disconnected(None);
                }
                _ => {}
            }
        });
    }

    fn on_event(event: plugin::widget_events::WidgetEvent) {
        with_plugin(|plugin| {
            if event.widget_id != modal::MODAL_ID {
                return;
            }
            match (&event.kind, &event.payload) {
                (plugin::widget_events::EventKind::Click, _) if event.node_key == modal::HANDLER_CANCEL => {
                    modal::close();
                }
                (plugin::widget_events::EventKind::Submit, plugin::widget_events::Payload::Fields(fields))
                    if event.node_key == modal::HANDLER_SUBMIT =>
                {
                    plugin.submit_connect(modal::Submitted::from_fields(fields));
                }
                _ => {}
            }
        });
    }

    fn on_layout(_layout: plugin::widget_events::WidgetLayout) {}
    // Nothing to clean up: the modal carries no state of its own between
    // events (see `modal.rs`), so a plugin-side close is a no-op.
    fn on_modal_closed(_id: String) {}
    // No confirm prompts yet.
    fn on_confirm(_id: String, _confirmed: bool) {}
}

// Public API other plugins import (`pato:pato-obs/scenes`) - see
// `pato-obs/wit/api.wit`. Thin: everything here just reads or forwards to
// `self.obs` (`obs.rs`), the same state the status widget and connect modal
// already drive.
impl scenes::Guest for PatoObs {
    fn list_scenes() -> Vec<scenes::Scene> {
        with_plugin(|plugin| plugin.obs.scenes.iter().map(to_wit_scene).collect())
    }

    fn current_program_scene() -> Option<scenes::Scene> {
        with_plugin(|plugin| {
            let name = plugin.obs.current_program_scene.as_ref()?;
            plugin.obs.scenes.iter().find(|s| &s.name == name).map(to_wit_scene)
        })
    }

    fn list_scene_items(scene: String) -> Vec<scenes::SceneItem> {
        with_plugin(|plugin| {
            // Lazily seed the per-scene cache on first access: the first
            // call for a scene nobody's asked about yet returns empty and
            // kicks off a fetch, exactly like `list-scenes` waits for the
            // initial connect sync (see PROTOCOL.md). Later calls just read
            // the already-live mirror.
            if !plugin.obs.scene_items.contains_key(&scene) && plugin.obs.is_connected() {
                plugin.obs.list_scene_items(&scene);
            }
            plugin
                .obs
                .scene_items
                .get(&scene)
                .map(|items| items.iter().map(to_wit_scene_item).collect())
                .unwrap_or_default()
        })
    }

    fn create_scene(name: String) -> bool {
        with_plugin(|plugin| plugin.obs.connected_then(|obs| obs.create_scene(&name)))
    }
    fn remove_scene(name: String) -> bool {
        with_plugin(|plugin| plugin.obs.connected_then(|obs| obs.remove_scene(&name)))
    }
    fn set_current_scene(name: String) -> bool {
        with_plugin(|plugin| plugin.obs.connected_then(|obs| obs.set_current_scene(&name)))
    }
    fn create_scene_item(scene: String, source: String) -> bool {
        with_plugin(|plugin| plugin.obs.connected_then(|obs| obs.create_scene_item(&scene, &source)))
    }
    fn remove_scene_item(scene: String, item_id: u32) -> bool {
        with_plugin(|plugin| plugin.obs.connected_then(|obs| obs.remove_scene_item(&scene, item_id)))
    }
    fn set_scene_item_enabled(scene: String, item_id: u32, enabled: bool) -> bool {
        with_plugin(|plugin| plugin.obs.connected_then(|obs| obs.set_scene_item_enabled(&scene, item_id, enabled)))
    }
}

fn to_wit_scene(s: &obs::Scene) -> scenes::Scene {
    scenes::Scene { name: s.name.clone(), uuid: s.uuid.clone() }
}
fn to_wit_scene_item(i: &obs::SceneItem) -> scenes::SceneItem {
    scenes::SceneItem { id: i.id, source_name: i.source_name.clone(), enabled: i.enabled }
}

impl plugin::websocket_events::Guest for PatoObs {
    fn on_message(id: String, data: String) {
        with_plugin(|plugin| {
            plugin.obs.on_message(&id, &data);
            plugin.sync_status();
        });
    }
    fn on_close(id: String, code: u16, reason: String) {
        with_plugin(|plugin| {
            plugin.obs.on_close(&id, format!("Disconnected ({code}): {reason}"));
            plugin.sync_status();
        });
    }
    fn on_error(id: String, message: String) {
        with_plugin(|plugin| {
            plugin.obs.on_close(&id, format!("Connection error: {message}"));
            plugin.sync_status();
        });
    }
}

impl PatoObs {
    fn init() -> Result<Self, String> {
        let mut plugin = PatoObs {
            status: widgets::ObsStatusWidget::new(),
            obs: obs::Obs::new(),
        };
        // Reconnect automatically if we have saved connection details from a
        // previous run - same pattern as the Twitch/Discord stored token.
        let (host, port, password) = plugin.saved_connection();
        if !host.is_empty() {
            if let Ok(port) = port.parse::<u16>() {
                plugin.obs.connect(&host, port, &password);
                plugin.status.connecting();
            }
        }
        Ok(plugin)
    }

    fn saved_connection(&self) -> (String, String, String) {
        (
            storage::get(KEY_HOST).unwrap_or_default(),
            storage::get(KEY_PORT).unwrap_or_else(|| modal::DEFAULT_PORT.to_string()),
            storage::get(KEY_PASSWORD).unwrap_or_default(),
        )
    }

    fn submit_connect(&mut self, form: modal::Submitted) {
        let host = form.host.trim().to_string();
        if host.is_empty() {
            modal::open(&host, &form.port, &form.password, Some("Enter a host."));
            return;
        }
        let port = match form.port.trim().parse::<u16>() {
            Ok(p) if p >= 1 => p,
            _ => {
                modal::open(&host, &form.port, &form.password, Some("Port must be a number between 1 and 65535."));
                return;
            }
        };
        storage::set(KEY_HOST, &host);
        storage::set(KEY_PORT, &port.to_string());
        storage::set(KEY_PASSWORD, &form.password);
        self.obs.connect(&host, port, &form.password);
        self.status.connecting();
        modal::close();
    }

    /// Reflect `self.obs`'s current connection state onto the status widget.
    /// Called after anything that can change it (`on_message`/`on_close`/`on_error`).
    fn sync_status(&mut self) {
        if self.obs.is_connected() {
            let host = storage::get(KEY_HOST).unwrap_or_default();
            self.status.connected(&host);
        } else if self.obs.is_connecting() {
            self.status.connecting();
        } else {
            self.status.disconnected(self.obs.last_error());
        }
    }
}

export!(PatoObs);
