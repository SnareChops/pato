//! OBS connection state machine, and the scene/scene-item API built on top of
//! it (`create_scene`, `remove_scene`, `set_current_scene`, `create_scene_item`,
//! `remove_scene_item`, `set_scene_item_enabled`).
//!
//! obs-websocket replies are not synchronous return values: a `Request` gets
//! its `RequestResponse` back later, through the same push channel
//! (`on-message`) as everything else - there is no blocking call to make here
//! the way `helix::TwitchApi` blocks on `wasi:http`. So every mutating method
//! below fires its request and returns immediately; the *result* shows up
//! through the mirrored `scenes` / `scene_items` state, which `on_message`
//! keeps in sync from OBS's event stream (`SceneCreated`, `SceneItemCreated`,
//! ...) - the same events fire regardless of who caused the change (Pato,
//! the OBS UI, or another controller), so that's the single source of truth
//! rather than hand-patching the cache per call. `pending` exists only to
//! populate the cache from the two `Get*List` responses (the one place
//! events alone don't cover: the *initial* snapshot on connect) and to log a
//! mutation's failure.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::pato::plugin::websocket;
use crate::protocol::{self, EventData, RequestResponseData};

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Scene {
    #[serde(rename = "sceneName")]
    pub name: String,
    #[serde(rename = "sceneUuid")]
    pub uuid: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SceneItem {
    #[serde(rename = "sceneItemId")]
    pub id: u32,
    #[serde(rename = "sourceName")]
    pub source_name: String,
    #[serde(rename = "sceneItemEnabled", default = "default_true")]
    pub enabled: bool,
}
fn default_true() -> bool {
    true
}

/// What a pending request's eventual `RequestResponse` should do. Only the
/// two list requests need to *do* anything on success (seed the cache) -
/// every mutating call's real effect arrives via the matching event instead
/// (see module docs), so this only needs to remember enough to log a
/// failure usefully.
enum Pending {
    ListScenes,
    ListSceneItems { scene: String },
    Mutation { label: String },
}

enum State {
    Disconnected { error: Option<String> },
    /// Socket open, `Hello`/`Identify`/`Identified` not finished yet.
    Handshaking { conn_id: String, password: String },
    Connected { conn_id: String },
}

pub struct Obs {
    state: State,
    next_id: u64,
    pending: HashMap<String, Pending>,
    pub current_program_scene: Option<String>,
    pub scenes: Vec<Scene>,
    pub scene_items: HashMap<String, Vec<SceneItem>>,
}

impl Obs {
    pub fn new() -> Self {
        Obs {
            state: State::Disconnected { error: None },
            next_id: 0,
            pending: HashMap::new(),
            current_program_scene: None,
            scenes: Vec::new(),
            scene_items: HashMap::new(),
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Connected { .. })
    }

    /// Runs `f` and returns `true` only when actually connected - the
    /// uniform "not connected, nothing was sent" guard every mutating
    /// request in the public `scenes` API (`lib.rs`) shares.
    pub fn connected_then(&mut self, f: impl FnOnce(&mut Self)) -> bool {
        if !self.is_connected() {
            return false;
        }
        f(self);
        true
    }
    pub fn is_connecting(&self) -> bool {
        matches!(self.state, State::Handshaking { .. })
    }
    pub fn last_error(&self) -> Option<&str> {
        match &self.state {
            State::Disconnected { error } => error.as_deref(),
            _ => None,
        }
    }

    /// `host`/`port` must already be a loopback address per the core's
    /// `ws://` policy - see `src-tauri/src/wasm/websocket.rs::is_permitted`.
    pub fn connect(&mut self, host: &str, port: u16, password: &str) {
        self.disconnect();
        match websocket::connect(&format!("ws://{host}:{port}")) {
            Ok(conn_id) => {
                self.state = State::Handshaking {
                    conn_id,
                    password: password.to_string(),
                };
            }
            Err(e) => self.state = State::Disconnected { error: Some(e) },
        }
    }

    pub fn disconnect(&mut self) {
        if let Some(id) = self.conn_id() {
            websocket::close(id);
        }
        self.reset(None);
    }

    fn conn_id(&self) -> Option<&str> {
        match &self.state {
            State::Handshaking { conn_id, .. } | State::Connected { conn_id } => Some(conn_id),
            State::Disconnected { .. } => None,
        }
    }

    fn reset(&mut self, error: Option<String>) {
        self.state = State::Disconnected { error };
        self.pending.clear();
        self.current_program_scene = None;
        self.scenes.clear();
        self.scene_items.clear();
    }

    // --- transport events (from `PatoObs::on_message` / `on_close` / `on_error`) ---

    pub fn on_close(&mut self, conn_id: &str, reason: String) {
        if self.conn_id() == Some(conn_id) {
            self.reset(Some(reason));
        }
    }

    pub fn on_message(&mut self, conn_id: &str, data: &str) {
        if self.conn_id() != Some(conn_id) {
            return; // a stale connection's tail messages after we moved on
        }
        let Ok(env) = serde_json::from_str::<protocol::Envelope>(data) else {
            return;
        };
        match env.op {
            protocol::OP_HELLO => self.on_hello(env.d),
            protocol::OP_IDENTIFIED => self.on_identified(),
            protocol::OP_EVENT => self.on_event(env.d),
            protocol::OP_REQUEST_RESPONSE => self.on_response(env.d),
            _ => {}
        }
    }

    fn on_hello(&mut self, d: Value) {
        let State::Handshaking { conn_id, password } = &self.state else {
            return;
        };
        let Ok(hello) = serde_json::from_value::<protocol::HelloData>(d) else {
            return;
        };
        let msg = protocol::identify(password, hello.authentication.as_ref());
        websocket::send(conn_id, &msg);
    }

    // `Identified.negotiatedRpcVersion` isn't inspected - we always request
    // rpcVersion 1 and the server has nothing else to negotiate down to.
    fn on_identified(&mut self) {
        let State::Handshaking { conn_id, .. } = &self.state else {
            return;
        };
        self.state = State::Connected {
            conn_id: conn_id.clone(),
        };
        self.list_scenes();
    }

    fn on_response(&mut self, d: Value) {
        let Ok(res) = serde_json::from_value::<RequestResponseData>(d) else {
            return;
        };
        let Some(pending) = self.pending.remove(&res.request_id) else {
            return;
        };
        if !res.request_status.result {
            let comment = res.request_status.comment.unwrap_or_default();
            let label = match &pending {
                Pending::ListScenes => "list scenes".to_string(),
                Pending::ListSceneItems { scene } => format!("list sources in \"{scene}\""),
                Pending::Mutation { label } => label.clone(),
            };
            eprintln!("obs-websocket: {label} failed: {comment}");
            return;
        }
        let Some(data) = res.response_data else {
            return;
        };
        match pending {
            Pending::ListScenes => {
                if let Some(scenes) = data.get("scenes").and_then(|v| v.as_array()) {
                    self.scenes = scenes
                        .iter()
                        .filter_map(|s| serde_json::from_value(s.clone()).ok())
                        .collect();
                }
                self.current_program_scene = data
                    .get("currentProgramSceneName")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
            Pending::ListSceneItems { scene } => {
                if let Some(items) = data.get("sceneItems").and_then(|v| v.as_array()) {
                    let items = items
                        .iter()
                        .filter_map(|s| serde_json::from_value(s.clone()).ok())
                        .collect();
                    self.scene_items.insert(scene, items);
                }
            }
            Pending::Mutation { .. } => {}
        }
    }

    fn on_event(&mut self, d: Value) {
        let Ok(ev) = serde_json::from_value::<EventData>(d) else {
            return;
        };
        let data = ev.event_data.unwrap_or(Value::Null);
        let str_field = |k: &str| data.get(k).and_then(|v| v.as_str()).map(str::to_string);

        match ev.event_type.as_str() {
            "SceneCreated" => {
                if let (Some(name), Some(uuid)) = (str_field("sceneName"), str_field("sceneUuid")) {
                    if !self.scenes.iter().any(|s| s.uuid == uuid) {
                        self.scenes.push(Scene { name, uuid });
                    }
                }
            }
            "SceneRemoved" => {
                if let Some(uuid) = str_field("sceneUuid") {
                    self.scenes.retain(|s| s.uuid != uuid);
                }
                if let Some(name) = str_field("sceneName") {
                    self.scene_items.remove(&name);
                }
            }
            "SceneNameChanged" => {
                if let (Some(uuid), Some(new_name)) = (str_field("sceneUuid"), str_field("sceneName")) {
                    if let Some(s) = self.scenes.iter_mut().find(|s| s.uuid == uuid) {
                        s.name = new_name;
                    }
                }
            }
            "SceneListChanged" => {
                if let Some(scenes) = data.get("scenes").and_then(|v| v.as_array()) {
                    self.scenes = scenes
                        .iter()
                        .filter_map(|s| serde_json::from_value(s.clone()).ok())
                        .collect();
                }
            }
            "CurrentProgramSceneChanged" => {
                self.current_program_scene = str_field("sceneName");
            }
            "SceneItemCreated" => {
                if let (Some(scene), Some(id)) = (str_field("sceneName"), data.get("sceneItemId").and_then(|v| v.as_u64()).map(|v| v as u32)) {
                    let source_name = str_field("sourceName").unwrap_or_default();
                    // The event doesn't carry the initial enabled state; OBS
                    // creates items enabled by default (and so does our own
                    // `create_scene_item`). A later `list_scene_items` call
                    // true's this up if it was actually created disabled.
                    self.scene_items.entry(scene).or_default().push(SceneItem {
                        id,
                        source_name,
                        enabled: true,
                    });
                }
            }
            "SceneItemRemoved" => {
                if let (Some(scene), Some(id)) = (str_field("sceneName"), data.get("sceneItemId").and_then(|v| v.as_u64()).map(|v| v as u32)) {
                    if let Some(items) = self.scene_items.get_mut(&scene) {
                        items.retain(|i| i.id != id);
                    }
                }
            }
            "SceneItemEnableStateChanged" => {
                if let (Some(scene), Some(id), Some(enabled)) = (
                    str_field("sceneName"),
                    data.get("sceneItemId").and_then(|v| v.as_u64()).map(|v| v as u32),
                    data.get("sceneItemEnabled").and_then(|v| v.as_bool()),
                ) {
                    if let Some(item) = self.scene_items.get_mut(&scene).and_then(|items| items.iter_mut().find(|i| i.id == id)) {
                        item.enabled = enabled;
                    }
                }
            }
            _ => {}
        }
    }

    // --- requests ------------------------------------------------------

    fn send(&mut self, request_type: &str, data: Option<Value>, pending: Pending) {
        let State::Connected { conn_id } = &self.state else {
            return;
        };
        self.next_id += 1;
        let id = self.next_id.to_string();
        let msg = protocol::request(request_type, &id, data);
        websocket::send(conn_id, &msg);
        self.pending.insert(id, pending);
    }

    pub fn list_scenes(&mut self) {
        self.send("GetSceneList", None, Pending::ListScenes);
    }

    pub fn list_scene_items(&mut self, scene: &str) {
        self.send(
            "GetSceneItemList",
            Some(json!({ "sceneName": scene })),
            Pending::ListSceneItems { scene: scene.to_string() },
        );
    }

    pub fn create_scene(&mut self, name: &str) {
        self.send(
            "CreateScene",
            Some(json!({ "sceneName": name })),
            Pending::Mutation { label: format!("create scene \"{name}\"") },
        );
    }
    pub fn remove_scene(&mut self, name: &str) {
        self.send(
            "RemoveScene",
            Some(json!({ "sceneName": name })),
            Pending::Mutation { label: format!("remove scene \"{name}\"") },
        );
    }
    pub fn set_current_scene(&mut self, name: &str) {
        self.send(
            "SetCurrentProgramScene",
            Some(json!({ "sceneName": name })),
            Pending::Mutation { label: format!("switch to scene \"{name}\"") },
        );
    }
    /// Adds an existing input (or scene, for nesting) as an item of `scene`.
    /// To create a brand new input in place, OBS's own `CreateInput` request
    /// does both steps atomically - add that wrapper once something needs it
    /// (e.g. the browser-source auto-creation design goal).
    pub fn create_scene_item(&mut self, scene: &str, source: &str) {
        self.send(
            "CreateSceneItem",
            Some(json!({ "sceneName": scene, "sourceName": source })),
            Pending::Mutation { label: format!("add \"{source}\" to \"{scene}\"") },
        );
    }
    pub fn remove_scene_item(&mut self, scene: &str, item_id: u32) {
        self.send(
            "RemoveSceneItem",
            Some(json!({ "sceneName": scene, "sceneItemId": item_id })),
            Pending::Mutation { label: format!("remove item {item_id} from \"{scene}\"") },
        );
    }
    pub fn set_scene_item_enabled(&mut self, scene: &str, item_id: u32, enabled: bool) {
        self.send(
            "SetSceneItemEnabled",
            Some(json!({ "sceneName": scene, "sceneItemId": item_id, "sceneItemEnabled": enabled })),
            Pending::Mutation { label: format!("{} item {item_id} in \"{scene}\"", if enabled { "enable" } else { "disable" }) },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> Obs {
        let mut obs = Obs::new();
        obs.state = State::Connected { conn_id: "c".into() };
        obs
    }

    #[test]
    fn seeds_scene_cache_from_list_response() {
        let mut obs = connected();
        obs.pending.insert("1".into(), Pending::ListScenes);
        obs.on_response(json!({
            "requestId": "1",
            "requestStatus": { "result": true, "code": 100 },
            "responseData": {
                "currentProgramSceneName": "Main",
                "scenes": [
                    { "sceneName": "Main", "sceneUuid": "u1" },
                    { "sceneName": "BRB", "sceneUuid": "u2" }
                ]
            }
        }));
        assert_eq!(obs.scenes.len(), 2);
        assert_eq!(obs.current_program_scene.as_deref(), Some("Main"));
    }

    #[test]
    fn a_failed_mutation_is_logged_and_does_not_touch_state() {
        let mut obs = connected();
        obs.scenes.push(Scene { name: "Main".into(), uuid: "u1".into() });
        obs.pending.insert("1".into(), Pending::Mutation { label: "x".into() });
        obs.on_response(json!({
            "requestId": "1",
            "requestStatus": { "result": false, "code": 601, "comment": "nope" },
        }));
        assert_eq!(obs.scenes.len(), 1); // unchanged
        assert!(obs.pending.is_empty()); // still consumed
    }

    #[test]
    fn scene_events_keep_the_cache_live_regardless_of_who_caused_them() {
        let mut obs = connected();
        obs.on_event(json!({ "eventType": "SceneCreated", "eventData": { "sceneName": "New", "sceneUuid": "u9" } }));
        assert_eq!(obs.scenes, vec![Scene { name: "New".into(), uuid: "u9".into() }]);

        obs.on_event(json!({ "eventType": "SceneNameChanged", "eventData": { "sceneUuid": "u9", "sceneName": "Renamed" } }));
        assert_eq!(obs.scenes[0].name, "Renamed");

        obs.on_event(json!({ "eventType": "CurrentProgramSceneChanged", "eventData": { "sceneName": "Renamed" } }));
        assert_eq!(obs.current_program_scene.as_deref(), Some("Renamed"));

        obs.on_event(json!({ "eventType": "SceneRemoved", "eventData": { "sceneName": "Renamed", "sceneUuid": "u9" } }));
        assert!(obs.scenes.is_empty());
    }

    #[test]
    fn scene_item_events_keep_the_per_scene_cache_live() {
        let mut obs = connected();
        obs.on_event(json!({
            "eventType": "SceneItemCreated",
            "eventData": { "sceneName": "Main", "sourceName": "Webcam", "sceneItemId": 5 }
        }));
        assert_eq!(obs.scene_items["Main"], vec![SceneItem { id: 5, source_name: "Webcam".into(), enabled: true }]);

        obs.on_event(json!({
            "eventType": "SceneItemEnableStateChanged",
            "eventData": { "sceneName": "Main", "sceneItemId": 5, "sceneItemEnabled": false }
        }));
        assert!(!obs.scene_items["Main"][0].enabled);

        obs.on_event(json!({
            "eventType": "SceneItemRemoved",
            "eventData": { "sceneName": "Main", "sceneItemId": 5 }
        }));
        assert!(obs.scene_items["Main"].is_empty());
    }

    #[test]
    fn ignores_messages_from_a_connection_that_is_no_longer_current() {
        let mut obs = connected();
        obs.on_message("stale", r#"{"op":5,"d":{"eventType":"SceneCreated","eventData":{"sceneName":"X","sceneUuid":"u"}}}"#);
        assert!(obs.scenes.is_empty());
    }
}
