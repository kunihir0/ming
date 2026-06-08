use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;
use crate::tracking::battlemetrics::types::{BmPlayer, BmServerPlayer, BmServerPlayerList};

#[derive(Clone)]
pub struct BmScraperClient {
    http: Client,
}

impl BmScraperClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }

    fn extract_bootstrap_json(html: &str) -> Option<serde_json::Value> {
        let start_marker = "id=\"storeBootstrap\" type=\"application/json\">";
        if let Some(start_idx) = html.find(start_marker) {
            let json_start = start_idx + start_marker.len();
            if let Some(end_idx) = html[json_start..].find("</script>") {
                let json_str = &html[json_start..json_start + end_idx];
                return serde_json::from_str(json_str).ok();
            }
        }
        None
    }

    /// Fetches a server page via the public API and parses all players currently online.
    pub async fn scrape_server_players(&self, server_id: &str) -> Result<BmServerPlayerList> {
        let url = format!("https://api.battlemetrics.com/servers/{}?include=player", server_id);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let json: serde_json::Value = resp.json().await?;
        
        let mut players = Vec::new();
        if let Some(included) = json.get("included").and_then(|v| v.as_array()) {
            for item in included {
                if item.get("type").and_then(|v| v.as_str()) == Some("player") {
                    let bm_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = item.pointer("/attributes/name").and_then(|v| v.as_str()).unwrap_or("").to_string();
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

    /// Fetches a player page and parses their online status and current name.
    pub async fn scrape_player_profile(&self, player_id: &str) -> Result<BmPlayer> {
        let url = format!("https://www.battlemetrics.com/players/{}", player_id);
        let html = self.http.get(&url).send().await?.error_for_status()?.text().await?;
        
        let mut current_name = player_id.to_string();
        let mut is_online = false;
        let mut current_server_id = None;

        if let Some(json) = Self::extract_bootstrap_json(&html) {
            if let Some(name) = json.pointer(&format!("/state/players/players/{}/name", player_id)).and_then(|v| v.as_str()) {
                current_name = name.to_string();
            } else {
                anyhow::bail!("storeBootstrap JSON was found, but player name was missing. The payload might be incomplete due to rate limits.");
            }

            if let Some(servers) = json.pointer(&format!("/state/players/serverInfo/{}", player_id)).and_then(|v| v.as_object()) {
                for (_, server_info) in servers {
                    if server_info.get("online").and_then(|v| v.as_bool()).unwrap_or(false) {
                        is_online = true;
                        current_server_id = server_info.get("serverId").and_then(|v| v.as_str()).map(|s| s.to_string());
                        break;
                    }
                }
            }
        } else {
            anyhow::bail!("No storeBootstrap JSON found on player profile page. Possibly rate limited or blocked.");
        }

        Ok(BmPlayer {
            bm_id: player_id.to_string(),
            current_name,
            is_online,
            current_server_id,
        })
    }

    pub async fn scrape_server_id_by_ip(&self, ip: &str) -> Result<Option<String>> {
        let url = format!("https://www.battlemetrics.com/servers/rust?q={}", ip);
        let html = self.http.get(&url).send().await?.error_for_status()?.text().await?;
        
        if let Some(json) = Self::extract_bootstrap_json(&html) {
            if let Some(servers) = json.pointer("/state/servers/servers").and_then(|v| v.as_object()) {
                for (_, server) in servers {
                    if let Some(server_ip) = server.get("ip").and_then(|v| v.as_str()) {
                        if server_ip == ip {
                            return Ok(server.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()));
                        }
                    }
                }
            }
        } else {
            anyhow::bail!("No storeBootstrap JSON found on search page. Possibly rate limited or blocked.");
        }
        
        Ok(None)
    }
}
