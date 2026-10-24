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

fn url_with_query(path: &str, params: &[(&str, &str)]) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params)
        .finish();
    format!("{}/{}?{}", API_URL, path, query)
}

// --- Channel point custom rewards ("redeems") -------------------------------

#[derive(serde::Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct MaxPerStream {
    pub is_enabled: bool,
    pub max_per_stream: u32,
}
#[derive(serde::Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct MaxPerUserPerStream {
    pub is_enabled: bool,
    pub max_per_user_per_stream: u32,
}
#[derive(serde::Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct GlobalCooldown {
    pub is_enabled: bool,
    pub global_cooldown_seconds: u32,
}

// A custom channel point reward as returned by Helix (images are ignored for now)
#[derive(serde::Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct Reward {
    pub id: String,
    pub title: String,
    pub cost: u32,
    pub prompt: String,
    pub is_enabled: bool,
    pub is_paused: bool,
    pub is_user_input_required: bool,
    pub should_redemptions_skip_request_queue: bool,
    pub max_per_stream_setting: MaxPerStream,
    pub max_per_user_per_stream_setting: MaxPerUserPerStream,
    pub global_cooldown_setting: GlobalCooldown,
    // Not part of Helix's payload: whether this app created the reward and so
    // may edit or delete it. Filled in by `list_rewards`.
    #[serde(skip)]
    pub manageable: bool,
}

#[derive(serde::Deserialize)]
struct RewardsResponse {
    data: Vec<Reward>,
}

// The writable fields of a reward. `None` fields are left out of the request
// body, so a PATCH containing only `is_paused` changes only that.
#[derive(serde::Serialize, Default, Debug, PartialEq)]
pub struct RewardInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_paused: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_user_input_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should_redemptions_skip_request_queue: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_max_per_stream_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_per_stream: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_max_per_user_per_stream_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_per_user_per_stream: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_global_cooldown_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_cooldown_seconds: Option<u32>,
}

// Turn a failed Helix response into a message fit to show the user.
fn reward_error(response: &http::Response) -> String {
    #[derive(serde::Deserialize)]
    struct Body {
        message: Option<String>,
    }
    let message = response
        .json::<Body>()
        .ok()
        .and_then(|b| b.message)
        .unwrap_or_else(|| response.text());
    match response.status {
        401 => "Twitch rejected the request. Pato may be missing the channel point permission: \
                disconnect and reconnect Twitch from its status widget."
            .to_string(),
        403 => "Twitch won't let Pato do that. Channel points need an Affiliate or Partner \
                channel, and Pato can only change redeems it created."
            .to_string(),
        404 => "That redeem no longer exists.".to_string(),
        _ if message.contains("DUPLICATE_REWARD") => {
            "You already have a redeem with that title.".to_string()
        }
        _ if message.contains("TOO_MANY_REWARDS") => {
            "Twitch allows at most 50 custom redeems per channel.".to_string()
        }
        _ => format!("Twitch error (HTTP {}): {}", response.status, message),
    }
}
pub struct TwitchApi {
    token: String,
    default_headers: types::Headers,
}
impl TwitchApi {
    // The raw oauth token, e.g. for IRC's `PASS oauth:<token>` login.
    pub fn token(&self) -> &str {
        &self.token
    }

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

    // All custom rewards on the channel, with `manageable` set for the ones
    // this app created (the only ones Helix lets it edit or delete).
    pub fn list_rewards(&self, broadcaster_id: &str) -> Result<Vec<Reward>, String> {
        let path = "channel_points/custom_rewards";
        let all = http::get(
            &url_with_query(path, &[("broadcaster_id", broadcaster_id)]),
            self.default_headers.clone(),
        )?;
        if !all.is_success() {
            return Err(reward_error(&all));
        }
        let manageable = http::get(
            &url_with_query(
                path,
                &[
                    ("broadcaster_id", broadcaster_id),
                    ("only_manageable_rewards", "true"),
                ],
            ),
            self.default_headers.clone(),
        )?;
        if !manageable.is_success() {
            return Err(reward_error(&manageable));
        }
        let manageable: Vec<String> = manageable
            .json::<RewardsResponse>()?
            .data
            .into_iter()
            .map(|r| r.id)
            .collect();
        let mut rewards = all.json::<RewardsResponse>()?.data;
        for reward in &mut rewards {
            reward.manageable = manageable.contains(&reward.id);
        }
        Ok(rewards)
    }

    pub fn create_reward(&self, broadcaster_id: &str, input: &RewardInput) -> Result<Reward, String> {
        let response = http::post(
            &url_with_query(
                "channel_points/custom_rewards",
                &[("broadcaster_id", broadcaster_id)],
            ),
            self.default_headers.clone(),
            serde_json::to_vec(input).map_err(|e| e.to_string())?,
        )?;
        self.reward_from(response)
    }

    pub fn update_reward(
        &self,
        broadcaster_id: &str,
        id: &str,
        input: &RewardInput,
    ) -> Result<Reward, String> {
        let response = http::patch(
            &url_with_query(
                "channel_points/custom_rewards",
                &[("broadcaster_id", broadcaster_id), ("id", id)],
            ),
            self.default_headers.clone(),
            serde_json::to_vec(input).map_err(|e| e.to_string())?,
        )?;
        self.reward_from(response)
    }

    pub fn delete_reward(&self, broadcaster_id: &str, id: &str) -> Result<(), String> {
        let response = http::delete(
            &url_with_query(
                "channel_points/custom_rewards",
                &[("broadcaster_id", broadcaster_id), ("id", id)],
            ),
            self.default_headers.clone(),
        )?;
        if response.is_success() {
            Ok(())
        } else {
            Err(reward_error(&response))
        }
    }

    // The single reward in a create/update response. Only rewards this app
    // created can be written to, so the result is always manageable.
    fn reward_from(&self, response: http::Response) -> Result<Reward, String> {
        if !response.is_success() {
            return Err(reward_error(&response));
        }
        let mut reward = response
            .json::<RewardsResponse>()?
            .data
            .into_iter()
            .next()
            .ok_or_else(|| "Twitch returned no reward".to_string())?;
        reward.manageable = true;
        Ok(reward)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quick_toggle_sends_only_the_changed_field() {
        let body = serde_json::to_string(&RewardInput {
            is_paused: Some(true),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(body, r#"{"is_paused":true}"#);
    }

    #[test]
    fn parses_a_helix_reward_and_tolerates_missing_fields() {
        let body = r#"{"data":[{
            "broadcaster_id":"1","id":"abc","title":"Hydrate","cost":500,
            "prompt":"drink","is_enabled":true,"is_paused":false,
            "is_user_input_required":false,
            "max_per_stream_setting":{"is_enabled":true,"max_per_stream":3},
            "global_cooldown_setting":{"is_enabled":false,"global_cooldown_seconds":0},
            "some_future_field":1
        }]}"#;
        let reward = serde_json::from_str::<RewardsResponse>(body).unwrap().data.remove(0);
        assert_eq!(reward.title, "Hydrate");
        assert_eq!(reward.cost, 500);
        assert!(reward.max_per_stream_setting.is_enabled);
        assert_eq!(reward.max_per_stream_setting.max_per_stream, 3);
        assert!(!reward.should_redemptions_skip_request_queue);
        assert!(!reward.manageable);
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(
            url_with_query("x", &[("id", "a b&c")]),
            "https://api.twitch.tv/helix/x?id=a+b%26c"
        );
    }
}
