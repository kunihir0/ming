use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::error::{Result, TeamDetectorError};
use crate::models::{BansInfo, SteamIdFriend};

const STEAM_ID_64_BASE: u64 = 76561197960265728;

pub fn account_id_to_steam_id64(account_id: u32) -> String {
    (account_id as u64 + STEAM_ID_64_BASE).to_string()
}

pub struct SteamIdDotComService {
    client: Client,
    debug: bool,
    request_delay: Duration,
    last_request_time: Arc<Mutex<Instant>>,
}

impl SteamIdDotComService {
    pub fn new(debug: bool) -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build()
                .unwrap_or_else(|_| Client::new()),
            debug,
            request_delay: Duration::from_secs(3),
            last_request_time: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(3))),
        }
    }

    fn log(&self, msg: &str) {
        if self.debug {
            println!("[SteamIdDotCom] {}", msg);
        }
    }

    async fn request(&self, url: &str) -> Result<String> {
        let mut last_req = self.last_request_time.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(*last_req);

        if elapsed < self.request_delay {
            let wait_time = self.request_delay - elapsed;
            self.log(&format!(
                "Rate limiting: Waiting {}ms...",
                wait_time.as_millis()
            ));
            tokio::time::sleep(wait_time).await;
        }

        *last_req = Instant::now();

        self.log(&format!("Requesting: {}", url));
        let response = self.client.get(url).send().await?.error_for_status()?;
        Ok(response.text().await?)
    }

    fn url_friends(&self, steam_id64: &str) -> String {
        format!("https://www.steamid.com/profiles/{}/friends", steam_id64)
    }

    /// Decodes the Astro island props serialization format
    fn decode_astro_value(val: &Value) -> Value {
        if let Value::Array(arr) = val
            && arr.len() == 2 {
                let type_tag = &arr[0];
                let inner_val = &arr[1];

                if let Value::Number(tag_num) = type_tag
                    && let Some(tag) = tag_num.as_u64() {
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
                            3 => return inner_val.clone(), // Date string
                            _ => return inner_val.clone(),
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

    pub async fn get_friends(&self, steam_id64: &str) -> Result<Vec<SteamIdFriend>> {
        let url = self.url_friends(steam_id64);
        let content = self.request(&url).await?;

        let document = Html::parse_document(&content);
        let island_sel = Selector::parse(r#"astro-island[component-url*="FriendsPage"]"#).unwrap();

        let island = document.select(&island_sel).next().ok_or_else(|| {
            TeamDetectorError::NotFound("No FriendsPage island found".to_string())
        })?;

        let props_attr = island
            .value()
            .attr("props")
            .ok_or_else(|| TeamDetectorError::NotFound("No props attribute found".to_string()))?;

        let raw_props: Value = serde_json::from_str(props_attr)
            .map_err(|e| TeamDetectorError::Parse(format!("Failed to parse JSON props: {}", e)))?;

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

                let account_id = f
                    .get("friend_account_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
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
                    depth: f.get("depth").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    friend_of: f.get("friend_of").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    bans: BansInfo {
                        community_banned: f
                            .get("community_banned")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        vac_bans: f
                            .get("number_of_vac_bans")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32,
                        game_bans: f
                            .get("number_of_game_bans")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32,
                        economy_ban: f
                            .get("economy_ban")
                            .and_then(|v| v.as_str())
                            .unwrap_or("none")
                            .to_string(),
                    },
                    mutual_friends: f
                        .get("mutual_friends")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_u64().map(|n| n as u32))
                                .collect()
                        })
                        .unwrap_or_default(),
                    total_friends: f
                        .get("total_friends")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32),
                };

                friends.push(friend);
            }
        }

        self.log(&format!(
            "get_friends({}) -> {} friends",
            steam_id64,
            friends.len()
        ));
        Ok(friends)
    }
}
