use crate::wasi::http::types;
use crate::{http, CLIENT_ID};

const API_URL: &str = "https://api.twitch.tv/helix";

#[derive(serde::Deserialize)]
struct TwitchUsersResponse {
    data: Vec<TwitchUser>,
}
#[derive(serde::Deserialize, Clone)]
pub struct TwitchUser {
    pub id: String,
    pub login: String,
    pub display_name: String,
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
    token: String,
    default_headers: types::Headers,
}
impl TwitchApi {
    pub fn new(token: String) -> Self {
        let default_headers = types::Headers::new();
        let _ = default_headers.append("Authorization", format!("Bearer {}", &token).as_bytes());
        let _ = default_headers.append("Client-Id", CLIENT_ID.as_bytes());
        let _ = default_headers.append("Content-Type", b"application/json");
        TwitchApi {
            token,
            default_headers,
        }
    }

    pub fn get_my_user(&self) -> Result<TwitchUser, String> {
        let response =
            http::get::<Vec<TwitchUser>>(url("/users").as_str(), self.default_headers.clone())?;
        if response.status != 200 {
            return Err(format!("Failed to get user. Code:{}", response.status));
        }
        match response.body {
            Some(users) => match users.first() {
                Some(user) => Ok(user.clone()),
                None => Err("No users returned".to_string()),
            },
            None => Err("No user data returned".to_string()),
        }
    }
}
