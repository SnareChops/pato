//! VTube Studio API message envelope and the authentication messages.
//! See `vtube-studio-api.md`.

use serde::Deserialize;
use serde_json::{json, Value};

pub const PLUGIN_NAME: &str = "Pato";
pub const PLUGIN_DEVELOPER: &str = "SnareChops";

pub const TYPE_API_ERROR: &str = "APIError";
pub const TYPE_TOKEN_RESPONSE: &str = "AuthenticationTokenResponse";
pub const TYPE_AUTH_RESPONSE: &str = "AuthenticationResponse";

/// The user clicked Deny on the plugin approval popup.
pub const ERROR_TOKEN_REQUEST_DENIED: i64 = 50;

#[derive(Deserialize)]
pub struct Envelope {
    #[serde(rename = "messageType")]
    pub message_type: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Deserialize)]
pub struct TokenResponse {
    #[serde(rename = "authenticationToken")]
    pub token: String,
}

#[derive(Deserialize)]
pub struct AuthResponse {
    pub authenticated: bool,
}

#[derive(Deserialize)]
pub struct ApiError {
    #[serde(rename = "errorID")]
    pub error_id: i64,
    #[serde(default)]
    pub message: String,
}

pub fn request(message_type: &str, data: Value) -> String {
    json!({
        "apiName": "VTubeStudioPublicAPI",
        "apiVersion": "1.0",
        "requestID": message_type,
        "messageType": message_type,
        "data": data,
    })
    .to_string()
}

/// Asks VTS to show the user an approval popup for this plugin.
pub fn token_request() -> String {
    request(
        "AuthenticationTokenRequest",
        json!({ "pluginName": PLUGIN_NAME, "pluginDeveloper": PLUGIN_DEVELOPER }),
    )
}

pub fn authentication_request(token: &str) -> String {
    request(
        "AuthenticationRequest",
        json!({
            "pluginName": PLUGIN_NAME,
            "pluginDeveloper": PLUGIN_DEVELOPER,
            "authenticationToken": token,
        }),
    )
}

// --- model widget requests ---

pub fn current_model() -> String {
    request("CurrentModelRequest", json!({}))
}

pub fn expression_state() -> String {
    request("ExpressionStateRequest", json!({}))
}

pub fn hotkeys_in_current_model() -> String {
    request("HotkeysInCurrentModelRequest", json!({}))
}

pub fn available_models() -> String {
    request("AvailableModelsRequest", json!({}))
}

/// An empty id unloads the current model. VTS enforces a 2s global cooldown.
pub fn model_load(model_id: &str) -> String {
    request("ModelLoadRequest", json!({ "modelID": model_id }))
}

pub fn face_found() -> String {
    request("FaceFoundRequest", json!({}))
}

pub fn subscribe(event: &str) -> String {
    request(
        "EventSubscriptionRequest",
        json!({ "eventName": event, "subscribe": true, "config": {} }),
    )
}

pub fn move_model(seconds: f64, x: f64, y: f64, rotation: f64, size: f64) -> String {
    request(
        "MoveModelRequest",
        json!({
            "timeInSeconds": seconds,
            "valuesAreRelativeToModel": false,
            "positionX": x,
            "positionY": y,
            "rotation": rotation,
            "size": size,
        }),
    )
}

pub fn hotkey_trigger(hotkey_id: &str) -> String {
    request("HotkeyTriggerRequest", json!({ "hotkeyID": hotkey_id }))
}

pub fn expression_activation(file: &str, active: bool) -> String {
    request(
        "ExpressionActivationRequest",
        json!({ "expressionFile": file, "fadeTime": 0.25, "active": active }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_request_carries_token_and_identity() {
        let v: Value = serde_json::from_str(&authentication_request("tok")).unwrap();
        assert_eq!(v["messageType"], "AuthenticationRequest");
        assert_eq!(v["data"]["authenticationToken"], "tok");
        assert_eq!(v["data"]["pluginName"], PLUGIN_NAME);
    }

    #[test]
    fn expression_activation_carries_file_and_state() {
        let v: Value = serde_json::from_str(&expression_activation("a.exp3.json", true)).unwrap();
        assert_eq!(v["messageType"], "ExpressionActivationRequest");
        assert_eq!(v["data"]["expressionFile"], "a.exp3.json");
        assert_eq!(v["data"]["active"], true);
    }

    #[test]
    fn model_load_carries_id() {
        let v: Value = serde_json::from_str(&model_load("abc")).unwrap();
        assert_eq!(v["messageType"], "ModelLoadRequest");
        assert_eq!(v["data"]["modelID"], "abc");
    }

    #[test]
    fn subscribe_names_the_event() {
        let v: Value = serde_json::from_str(&subscribe("ModelLoadedEvent")).unwrap();
        assert_eq!(v["data"]["eventName"], "ModelLoadedEvent");
        assert_eq!(v["data"]["subscribe"], true);
    }

    #[test]
    fn parses_error_envelope() {
        let env: Envelope = serde_json::from_str(
            r#"{"messageType":"APIError","data":{"errorID":50,"message":"denied"}}"#,
        )
        .unwrap();
        assert_eq!(env.message_type, TYPE_API_ERROR);
        let err: ApiError = serde_json::from_value(env.data).unwrap();
        assert_eq!(err.error_id, ERROR_TOKEN_REQUEST_DENIED);
    }
}
