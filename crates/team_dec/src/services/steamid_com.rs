use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::error::{Result, TeamDetectorError};
use crate::models::{BansInfo, SteamIdFriend};

const STEAM_ID_64_BASE: u64 = 76561197960265728;

/// Converts a Steam account ID (32-bit) to a Steam ID 64.
#[must_use]
pub fn account_id_to_steam_id64(account_id: u32) -> String {
    (u64::from(account_id) + STEAM_ID_64_BASE).to_string()
}

static SELECTOR_ISLAND: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(r#"astro-island[component-url*="FriendsPage"]"#).unwrap());

pub struct SteamIdDotComService {
    client: Client,
    debug: bool,
    request_delay: Duration,
    last_request_time: Arc<Mutex<Instant>>,
}

impl SteamIdDotComService {
    /// Create a new `SteamIdComService`.
    #[must_use]
    pub fn new(debug: bool) -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build()
                .unwrap_or_else(|_| Client::new()),
            debug,
            request_delay: Duration::from_secs(3),
            last_request_time: Arc::new(Mutex::new(Instant::now().checked_sub(Duration::from_secs(3)).unwrap_or_else(Instant::now))),
        }
    }

    async fn request(&self, url: &str) -> Result<String> {
        let wait_time = {
            let mut last_req = self.last_request_time.lock().await;
            let now = Instant::now();
            let target = *last_req + self.request_delay;

            if target > now {
                let wait = target.duration_since(now);
                *last_req = target;
                Some(wait)
            } else {
                *last_req = now;
                None
            }
        };

        if let Some(wait) = wait_time {
            if self.debug {
                tracing::debug!(wait_ms = wait.as_millis(), "Rate limiting: Waiting...");
            }
            tokio::time::sleep(wait).await;
        }

        if self.debug {
            tracing::debug!(url = %url, "Requesting");
        }
        let response = self.client.get(url).send().await?.error_for_status()?;
        Ok(response.text().await?)
    }

    #[must_use]
    fn url_friends(&self, steam_id64: &str) -> String {
        format!("https://www.steamid.com/profiles/{steam_id64}/friends")
    }

    /// Decodes the Astro island props serialization format
    #[allow(clippy::collapsible_if, clippy::match_same_arms)] // Keeping nested logic for clarity with astro components
    fn decode_astro_value(val: &Value) -> Value {
        if let Value::Array(arr) = val {
            if arr.len() == 2 {
                let type_tag = &arr[0];
                let inner_val = &arr[1];

                if let Value::Number(tag_num) = type_tag {
                    if let Some(tag) = tag_num.as_u64() {
                        match tag {
                            0 => return inner_val.clone(), // Primitive
                            1 => {
                                // Array of tagged values
                                if let Value::Array(inner_arr) = inner_val {
                                    let decoded: Vec<Value> =
                                        inner_arr.iter().map(Self::decode_astro_value).collect();
                                    return Value::Array(decoded);
                                }
                                return inner_val.clone();
                            }
                            2 => return inner_val.clone(), // Regexp
                            3 => return inner_val.clone(), // Date string
                            _ => return inner_val.clone(),
                        }
                    }
                }
            }
        }
        val.clone()
    }

    fn decode_astro_object(obj: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
        let mut decoded = serde_json::Map::new();
        for (k, v) in obj {
            decoded.insert(k.clone(), Self::decode_astro_value(v));
        }
        decoded
    }

    /// Fetches friends list from steamid.com.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails, or if parsing the Astro Island payload fails.
    #[allow(clippy::too_many_lines)] // Allowed to keep all parsing logic localized
    pub async fn get_friends(&self, steam_id64: &str) -> Result<Vec<SteamIdFriend>> {
        let url = self.url_friends(steam_id64);
        let content = self.request(&url).await?;

        let document = Html::parse_document(&content);

        let island = document.select(&SELECTOR_ISLAND).next().ok_or_else(|| {
            TeamDetectorError::NotFound("No FriendsPage island found".to_string())
        })?;

        let props_attr = island
            .value()
            .attr("props")
            .ok_or_else(|| TeamDetectorError::NotFound("No props attribute found".to_string()))?;

        let raw_props: Value = serde_json::from_str(props_attr)
            .map_err(|e| TeamDetectorError::Parse(format!("Failed to parse JSON props: {e}")))?;

        let decoded_props = match raw_props {
            Value::Object(map) => Self::decode_astro_object(&map),
            _ => {
                return Err(TeamDetectorError::Parse(
                    "Props is not an object".to_string(),
                ));
            }
        };

        let friends_data = decoded_props
            .get("friendsData")
            .and_then(|v| v.as_object())
            .ok_or_else(|| TeamDetectorError::NotFound("No friendsData in props".to_string()))?;

        let friends_raw = friends_data
            .get("friends")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                TeamDetectorError::NotFound("No friends array in friendsData".to_string())
            })?;

        let mut friends = Vec::new();
        for raw_friend in friends_raw {
            if let Value::Object(map) = raw_friend {
                let f = Self::decode_astro_object(map);

                let account_id = u32::try_from(
                    f.get("friend_account_id")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                )
                .unwrap_or(0);
                if account_id == 0 {
                    continue;
                }

                let friend = SteamIdFriend {
                    account_id,
                    steam_id64: account_id_to_steam_id64(account_id),
                    persona_name: f
                        .get("persona_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    privacy_state: f
                        .get("privacy_state")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    friend_since: f
                        .get("added")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    member_since: f
                        .get("member_since")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    depth: u32::try_from(
                        f.get("depth")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                    )
                    .unwrap_or(0),
                    friend_of: u32::try_from(
                        f.get("friend_of")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                    )
                    .unwrap_or(0),
                    bans: BansInfo {
                        community_banned: f
                            .get("community_banned")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                        vac_bans: u32::try_from(
                            f.get("number_of_vac_bans")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0),
                        )
                        .unwrap_or(0),
                        game_bans: u32::try_from(
                            f.get("number_of_game_bans")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0),
                        )
                        .unwrap_or(0),
                        economy_ban: f
                            .get("economy_ban")
                            .and_then(|v| v.as_str())
                            .unwrap_or("none")
                            .to_string(),
                    },
                    mutual_friends: f
                        .get("mutual_friends")
                        .and_then(serde_json::Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_u64().map(|n| u32::try_from(n).unwrap_or(0)))
                                .collect()
                        })
                        .unwrap_or_default(),
                    total_friends: f
                        .get("total_friends")
                        .and_then(serde_json::Value::as_u64)
                        .map(|v| u32::try_from(v).unwrap_or(0)),
                };

                friends.push(friend);
            }
        }

        if self.debug {
            tracing::debug!(
                steam_id64 = %steam_id64,
                friends_count = friends.len(),
                "get_friends result"
            );
        }
        Ok(friends)
    }
}
