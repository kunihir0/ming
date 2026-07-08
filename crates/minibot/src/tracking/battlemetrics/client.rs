use crate::tracking::battlemetrics::types::{BmPlayer, BmServerPlayer, BmServerPlayerList};
use anyhow::Result;
use reqwest::Client;
use std::time::Duration;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct BmScraperClient {
    http: Client,
    server_id_cache: Arc<RwLock<HashMap<String, String>>>,
    server_id_failures: Arc<RwLock<HashMap<String, Instant>>>,
}

impl BmScraperClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            server_id_cache: Arc::new(RwLock::new(HashMap::new())),
            server_id_failures: Arc::new(RwLock::new(HashMap::new())),
        }
    }



    /// Fetches a server page via the public API and parses all players currently online.
    pub async fn scrape_server_players(&self, server_id: &str) -> Result<BmServerPlayerList> {
        let url = format!(
            "https://api.battlemetrics.com/servers/{}?include=player",
            server_id
        );
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let json: serde_json::Value = resp.json().await?;

        let mut players = Vec::new();
        if let Some(included) = json.get("included").and_then(|v| v.as_array()) {
            for item in included {
                if item.get("type").and_then(|v| v.as_str()) == Some("player") {
                    let bm_id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = item
                        .pointer("/attributes/name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !bm_id.is_empty() {
                        players.push(BmServerPlayer { bm_id, name });
                    }
                }
            }
        }

        Ok(BmServerPlayerList {
            server_id: server_id.to_string(),
            players,
        })
    }

    /// Fetches a player page via the official API to avoid Cloudflare blocks.
    pub async fn scrape_player_profile(&self, player_id: &str) -> Result<BmPlayer> {
        let url = format!(
            "https://api.battlemetrics.com/players/{}?include=server",
            player_id
        );
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let json: serde_json::Value = resp.json().await?;

        let mut current_name = player_id.to_string();
        if let Some(name) = json
            .pointer("/data/attributes/name")
            .and_then(|v| v.as_str())
        {
            current_name = name.to_string();
        }

        let mut is_online = false;
        let mut current_server_id = None;
        let mut total_playtime_seconds = 0;

        if let Some(included) = json.get("included").and_then(|v| v.as_array()) {
            for item in included {
                if item.get("type").and_then(|v| v.as_str()) == Some("server") {
                    if let Some(meta) = item.get("meta") {
                        if let Some(time) = meta.get("timePlayed").and_then(|v| v.as_u64()) {
                            total_playtime_seconds += time;
                        }
                        if meta
                            .get("online")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                        {
                            is_online = true;
                            current_server_id = item
                                .get("id")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                        }
                    }
                }
            }
        }

        Ok(BmPlayer {
            bm_id: player_id.to_string(),
            current_name,
            is_online,
            current_server_id,
            total_playtime_seconds,
        })
    }

    pub async fn scrape_server_id_by_ip(&self, ip: &str) -> Result<Option<String>> {
        {
            let cache = self.server_id_cache.read().await;
            if let Some(id) = cache.get(ip) {
                return Ok(Some(id.clone()));
            }
        }

        {
            let failures = self.server_id_failures.read().await;
            if let Some(time) = failures.get(ip) {
                if time.elapsed() < Duration::from_secs(300) {
                    return Ok(None); // Still on cooldown
                }
            }
        }

        let url = format!(
            "https://api.battlemetrics.com/servers?filter[search]={}&filter[game]=rust",
            ip
        );
        let resp_res = self.http.get(&url).send().await;

        let handle_failure = || async {
            self.server_id_failures
                .write()
                .await
                .insert(ip.to_string(), Instant::now());
        };

        let resp = match resp_res {
            Ok(r) => {
                let r = match r.error_for_status() {
                    Ok(r) => r,
                    Err(e) => {
                        handle_failure().await;
                        return Err(e.into());
                    }
                };
                r
            }
            Err(e) => {
                handle_failure().await;
                return Err(e.into());
            }
        };

        let json: serde_json::Value = resp.json().await?;

        if let Some(data) = json.get("data").and_then(|v| v.as_array()) {
            for server in data {
                if let Some(server_ip) = server.pointer("/attributes/ip").and_then(|v| v.as_str()) {
                    if server_ip == ip {
                        if let Some(id) = server
                            .pointer("/attributes/id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                        {
                            self.server_id_cache
                                .write()
                                .await
                                .insert(ip.to_string(), id.clone());
                            return Ok(Some(id));
                        }
                    }
                }
            }
        }

        handle_failure().await;
        Ok(None)
    }

    pub async fn get_server_name(&self, server_id: &str) -> Result<String> {
        let url = format!("https://api.battlemetrics.com/servers/{}", server_id);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let json: serde_json::Value = resp.json().await?;

        if let Some(name) = json
            .pointer("/data/attributes/name")
            .and_then(|v| v.as_str())
        {
            Ok(name.to_string())
        } else {
            anyhow::bail!("Server name not found in response");
        }
    }
}
