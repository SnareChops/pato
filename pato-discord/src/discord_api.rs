use crate::wasi::http::types;
use crate::http;

const API_URL: &str = "https://discord.com/api/v10";
const CDN_URL: &str = "https://cdn.discordapp.com";

#[derive(serde::Deserialize, Clone)]
// Full Discord "Get Current User" response model; not every field is consumed yet
#[allow(dead_code)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    /// Legacy per-username suffix. Migrated accounts report "0" and are
    /// identified solely by `username` / `global_name` instead.
    pub discriminator: String,
    /// Display name under Discord's newer username system; falls back to
    /// `username` when unset (bots, accounts not yet migrated).
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub email: Option<String>,
    pub verified: Option<bool>,
}

impl DiscordUser {
    pub fn display_name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }

    // Resolves to the user's uploaded avatar, or one of Discord's default
    // avatars when they haven't set one.
    pub fn avatar_url(&self) -> String {
        match &self.avatar {
            Some(hash) => {
                let ext = if hash.starts_with("a_") { "gif" } else { "png" };
                format!("{CDN_URL}/avatars/{}/{hash}.{ext}", self.id)
            }
            None => {
                let index = default_avatar_index(&self.id, &self.discriminator);
                format!("{CDN_URL}/embed/avatars/{index}.png")
            }
        }
    }
}

// Legacy discriminators (pre-username-migration) pick a default avatar by
// `discriminator % 5`; migrated accounts (discriminator "0") use the newer
// `(id >> 22) % 6` scheme. See Discord's "Default Avatars" docs.
fn default_avatar_index(id: &str, discriminator: &str) -> u64 {
    if discriminator != "0" {
        discriminator.parse::<u64>().unwrap_or(0) % 5
    } else {
        (id.parse::<u64>().unwrap_or(0) >> 22) % 6
    }
}

fn url(path: &str) -> String {
    format!("{API_URL}/{path}")
}

pub struct DiscordApi {
    default_headers: types::Headers,
}
impl DiscordApi {
    pub fn new(token: String) -> Self {
        let default_headers = types::Headers::from_list(&[(
            "Authorization".to_string(),
            format!("Bearer {token}").into_bytes(),
        )])
        .expect("valid default headers");
        DiscordApi { default_headers }
    }

    pub fn get_my_user(&self) -> Result<DiscordUser, String> {
        let response = http::get(&url("users/@me"), self.default_headers.clone())?;
        if !response.is_success() {
            return Err(format!(
                "Failed to get user (HTTP {}): {}",
                response.status,
                response.text()
            ));
        }
        response.json::<DiscordUser>()
    }
}
