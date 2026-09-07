use crate::wasi::http::types;
use crate::{http, CLIENT_ID};

const API_URL: &str = "https://api.twitch.tv/helix";

#[derive(serde::Deserialize)]
struct TwitchUsersResponse {
    data: Vec<TwitchUser>,
}
#[derive(serde::Deserialize, Clone)]
// Full Twitch "Get Users" response model; not every field is consumed yet
#[allow(dead_code)]
pub struct TwitchUser {
    pub id: String,
    pub login: String,
    pub display_name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub broadcaster_type: String,
    pub description: String,
    pub profile_image_url: String,
    pub offline_image_url: String,
    pub email: Option<String>,
    pub created_at: String,
}
fn url(path: &str) -> String {
    format!("{}/{}", API_URL, path)
}
pub struct TwitchApi {
    // Retained for the upcoming token-refresh flow
    #[allow(dead_code)]
    token: String,
    default_headers: types::Headers,
}
impl TwitchApi {
    pub fn new(token: String) -> Self {
        let default_headers = types::Headers::from_list(&[
            (
                "Authorization".to_string(),
                format!("Bearer {}", token).into_bytes(),
            ),
            ("Client-Id".to_string(), CLIENT_ID.as_bytes().to_vec()),
            ("Content-Type".to_string(), b"application/json".to_vec()),
        ])
        .expect("valid default headers");
        TwitchApi {
            token,
            default_headers,
        }
    }

    pub fn get_my_user(&self) -> Result<TwitchUser, String> {
        let response = http::get(&url("users"), self.default_headers.clone())?;
        if !response.is_success() {
            return Err(format!(
                "Failed to get user (HTTP {}): {}",
                response.status,
                response.text()
            ));
        }
        response
            .json::<TwitchUsersResponse>()?
            .data
            .into_iter()
            .next()
            .ok_or_else(|| "No users returned".to_string())
    }
}
