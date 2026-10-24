//! Mirror of what VTube Studio reports about the currently loaded model:
//! name, position, tracking, expressions and hotkeys. `apply` folds each
//! response or event in; the widget renders from it.

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Position {
    #[serde(rename = "positionX")]
    pub x: f64,
    #[serde(rename = "positionY")]
    pub y: f64,
    pub rotation: f64,
    pub size: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Expression {
    pub name: String,
    pub file: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Hotkey {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(rename = "hotkeyID")]
    pub id: String,
}

/// A model VTS can load.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Available {
    #[serde(rename = "modelID")]
    pub id: String,
    #[serde(rename = "modelName", default)]
    pub name: String,
}

/// `ToggleExpression` hotkeys are the expressions section's buttons already.
const HOTKEY_TYPE_TOGGLE_EXPRESSION: &str = "ToggleExpression";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub loaded: bool,
    pub name: String,
    id: String,
    pub position: Option<Position>,
    pub face_found: Option<bool>,
    pub expressions: Vec<Expression>,
    pub hotkeys: Vec<Hotkey>,
    /// Every model VTS can load, for the switcher.
    pub available: Vec<Available>,
}

/// What folding a message in did.
#[derive(Debug, Default, PartialEq)]
pub struct Applied {
    /// Something the widget shows is different now.
    pub changed: bool,
    /// The loaded model (or its config) changed: re-fetch everything.
    pub refresh: bool,
}

impl Model {
    pub fn reset(&mut self) {
        *self = Model::default();
    }

    /// The loaded model's VTS id; empty when none is loaded.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Hotkeys worth a button of their own.
    pub fn extra_hotkeys(&self) -> impl Iterator<Item = &Hotkey> {
        self.hotkeys
            .iter()
            .filter(|h| h.kind != HOTKEY_TYPE_TOGGLE_EXPRESSION && !h.id.is_empty())
    }

    /// Flip an expression locally right after the user clicked it, so the
    /// button responds before the next poll confirms it.
    pub fn set_expression_active(&mut self, file: &str, active: bool) {
        if let Some(e) = self.expressions.iter_mut().find(|e| e.file == file) {
            e.active = active;
        }
    }

    pub fn apply(&mut self, message_type: &str, data: Value) -> Applied {
        let before = self.clone();
        let mut refresh = false;
        match message_type {
            "CurrentModelResponse" => refresh = self.on_current_model(data),
            "AvailableModelsResponse" => self.on_available(data),
            "ExpressionStateResponse" => self.on_expressions(data),
            "HotkeysInCurrentModelResponse" => self.on_hotkeys(data),
            "FaceFoundResponse" => {
                self.face_found = data.get("found").and_then(Value::as_bool);
            }
            "TrackingStatusChangedEvent" => {
                self.face_found = data.get("faceFound").and_then(Value::as_bool);
            }
            "ModelLoadedEvent" | "ModelConfigChangedEvent" => refresh = true,
            _ => {}
        }
        Applied { changed: *self != before, refresh }
    }

    /// Returns whether a different model is now loaded.
    fn on_current_model(&mut self, data: Value) -> bool {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "modelLoaded")]
            loaded: bool,
            #[serde(rename = "modelName", default)]
            name: String,
            #[serde(rename = "modelID", default)]
            id: String,
            #[serde(rename = "modelPosition")]
            position: Option<Position>,
        }
        let Ok(res) = serde_json::from_value::<Res>(data) else {
            return false;
        };
        if !res.loaded {
            let had_model = self.loaded;
            let face = self.face_found;
            let available = std::mem::take(&mut self.available);
            self.reset();
            self.face_found = face;
            self.available = available;
            return had_model;
        }
        let switched = res.id != self.id;
        if switched {
            self.expressions.clear();
            self.hotkeys.clear();
        }
        self.loaded = true;
        self.name = res.name;
        self.id = res.id;
        self.position = res.position;
        switched
    }

    fn on_available(&mut self, data: Value) {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "availableModels", default)]
            models: Vec<Available>,
        }
        if let Ok(res) = serde_json::from_value::<Res>(data) {
            self.available = res.models;
        }
    }

    fn on_expressions(&mut self, data: Value) {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "modelLoaded")]
            loaded: bool,
            #[serde(default)]
            expressions: Vec<Expression>,
        }
        if let Ok(res) = serde_json::from_value::<Res>(data) {
            self.expressions = if res.loaded { res.expressions } else { vec![] };
        }
    }

    fn on_hotkeys(&mut self, data: Value) {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "modelLoaded")]
            loaded: bool,
            #[serde(rename = "availableHotkeys", default)]
            hotkeys: Vec<Hotkey>,
        }
        if let Ok(res) = serde_json::from_value::<Res>(data) {
            self.hotkeys = if res.loaded { res.hotkeys } else { vec![] };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn current(id: &str, name: &str) -> Value {
        json!({ "modelLoaded": true, "modelName": name, "modelID": id,
                "modelPosition": { "positionX": 0.5, "positionY": -0.25, "rotation": 0.0, "size": -20.0 } })
    }

    #[test]
    fn first_model_requests_refresh_then_settles() {
        let mut m = Model::default();
        let a = m.apply("CurrentModelResponse", current("id1", "Pato"));
        assert!(a.changed && a.refresh);
        assert_eq!(m.name, "Pato");
        // Polling the same model again: nothing changes, no refresh.
        let a = m.apply("CurrentModelResponse", current("id1", "Pato"));
        assert_eq!(a, Applied::default());
    }

    #[test]
    fn switching_models_clears_stale_expressions() {
        let mut m = Model::default();
        m.apply("CurrentModelResponse", current("id1", "A"));
        m.apply(
            "ExpressionStateResponse",
            json!({ "modelLoaded": true, "expressions": [{ "name": "Blush", "file": "b.exp3.json", "active": true }] }),
        );
        assert_eq!(m.expressions.len(), 1);
        let a = m.apply("CurrentModelResponse", current("id2", "B"));
        assert!(a.refresh);
        assert!(m.expressions.is_empty());
    }

    #[test]
    fn unloading_keeps_tracking_state() {
        let mut m = Model::default();
        m.apply("CurrentModelResponse", current("id1", "A"));
        m.apply("TrackingStatusChangedEvent", json!({ "faceFound": true }));
        m.apply("CurrentModelResponse", json!({ "modelLoaded": false }));
        assert!(!m.loaded);
        assert_eq!(m.face_found, Some(true));
    }

    #[test]
    fn available_models_survive_unloading() {
        let mut m = Model::default();
        m.apply(
            "AvailableModelsResponse",
            json!({ "availableModels": [{ "modelID": "a", "modelName": "A" }, { "modelID": "b", "modelName": "B" }] }),
        );
        m.apply("CurrentModelResponse", current("a", "A"));
        m.apply("CurrentModelResponse", json!({ "modelLoaded": false }));
        assert_eq!(m.available.len(), 2);
        m.reset();
        assert!(m.available.is_empty());
    }

    #[test]
    fn model_events_request_refresh() {
        let mut m = Model::default();
        assert!(m.apply("ModelLoadedEvent", json!({})).refresh);
        assert!(m.apply("ModelConfigChangedEvent", json!({})).refresh);
    }

    #[test]
    fn toggle_expression_hotkeys_are_not_extra() {
        let mut m = Model::default();
        m.apply(
            "HotkeysInCurrentModelResponse",
            json!({ "modelLoaded": true, "availableHotkeys": [
                { "name": "Blush", "type": "ToggleExpression", "hotkeyID": "1" },
                { "name": "Wave", "type": "TriggerAnimation", "hotkeyID": "2" } ] }),
        );
        let names: Vec<_> = m.extra_hotkeys().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["Wave"]);
    }
}
