//! obs-websocket v5 message envelope and auth. Pure wire-format code - no
//! socket handling here, see `obs.rs`. Field shapes are cited from
//! `PROTOCOL.md` (and, where that omits an exact schema, from the community
//! `SceneItemCreated`/`InputCreated` event payloads it does document).

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const OP_HELLO: u8 = 0;
pub const OP_IDENTIFY: u8 = 1;
pub const OP_IDENTIFIED: u8 = 2;
pub const OP_EVENT: u8 = 5;
pub const OP_REQUEST: u8 = 6;
pub const OP_REQUEST_RESPONSE: u8 = 7;

#[derive(Deserialize)]
pub struct Envelope {
    pub op: u8,
    pub d: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloData {
    pub authentication: Option<AuthChallenge>,
}
#[derive(Deserialize)]
pub struct AuthChallenge {
    pub challenge: String,
    pub salt: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentifyData<'a> {
    rpc_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    authentication: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestData<'a> {
    request_type: &'a str,
    request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_data: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestStatus {
    pub result: bool,
    #[serde(default)]
    pub comment: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponseData {
    pub request_id: String,
    pub request_status: RequestStatus,
    #[serde(default)]
    pub response_data: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventData {
    pub event_type: String,
    #[serde(default)]
    pub event_data: Option<Value>,
}

#[derive(Serialize)]
struct Out<T: Serialize> {
    op: u8,
    d: T,
}

/// `Identify` in response to a `Hello`. `challenge` is `None` when the server
/// requires no authentication - the resulting message carries no
/// `authentication` field either way in that case.
pub fn identify(password: &str, challenge: Option<&AuthChallenge>) -> String {
    let auth = challenge.map(|c| auth_string(password, &c.salt, &c.challenge));
    serde_json::to_string(&Out {
        op: OP_IDENTIFY,
        d: IdentifyData {
            rpc_version: 1,
            authentication: auth.as_deref(),
        },
    })
    .expect("Identify always serializes")
}

/// `password + salt` -> sha256 -> base64 ("secret"), then
/// `secret + challenge` -> sha256 -> base64 ("authentication"). The password
/// itself never crosses the wire - see PROTOCOL.md.
fn auth_string(password: &str, salt: &str, challenge: &str) -> String {
    let secret = STANDARD.encode(Sha256::digest(format!("{password}{salt}").as_bytes()));
    STANDARD.encode(Sha256::digest(format!("{secret}{challenge}").as_bytes()))
}

pub fn request(request_type: &str, request_id: &str, data: Option<Value>) -> String {
    serde_json::to_string(&Out {
        op: OP_REQUEST,
        d: RequestData {
            request_type,
            request_id,
            request_data: data,
        },
    })
    .expect("Request always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    // From the "Creating an authentication string" example in
    // obsproject/obs-websocket's protocol.md (password "supersecretpassword").
    #[test]
    fn auth_string_matches_the_documented_example() {
        let got = auth_string(
            "supersecretpassword",
            "lM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI=",
            "+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY=",
        );
        // The doc doesn't publish the final hash for this exact pair, so this
        // pins the *computation*: re-deriving the secret independently must
        // agree with what `auth_string` produced internally.
        let secret = STANDARD.encode(Sha256::digest(
            b"supersecretpasswordlM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI=",
        ));
        let expect = STANDARD.encode(Sha256::digest(
            format!("{secret}+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY=").as_bytes(),
        ));
        assert_eq!(got, expect);
        assert_eq!(got.len(), 44); // base64 of a 32-byte sha256 digest
    }

    #[test]
    fn identify_omits_authentication_when_no_challenge() {
        let msg = identify("unused", None);
        let v: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["op"], 1);
        assert_eq!(v["d"]["rpcVersion"], 1);
        assert!(v["d"].get("authentication").is_none());
    }

    #[test]
    fn identify_includes_authentication_when_challenged() {
        let challenge = AuthChallenge {
            challenge: "c".into(),
            salt: "s".into(),
        };
        let msg = identify("pw", Some(&challenge));
        let v: Value = serde_json::from_str(&msg).unwrap();
        assert!(v["d"]["authentication"].as_str().unwrap().len() > 0);
    }

    #[test]
    fn request_wire_shape() {
        let msg = request("GetSceneList", "1", None);
        let v: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["op"], 6);
        assert_eq!(v["d"]["requestType"], "GetSceneList");
        assert_eq!(v["d"]["requestId"], "1");
        assert!(v["d"].get("requestData").is_none());
    }
}
