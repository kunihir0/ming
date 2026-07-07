use anyhow::{Context, Result};
use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize)]
pub struct AtlasPlayerResponse {
    pub player: Option<AtlasPlayer>,
    pub lookup_quota: Option<AtlasLookupQuota>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AtlasPlayer {
    pub id: String,
    pub name: String,
    pub steam_id: String,
    pub bm_hours: i64,
    pub atlas_hours: i64,
    pub bm_player_id: Option<i64>,
    pub online_server_id: Option<i64>,
    pub last_online: Option<String>,
    pub is_banned: bool,
    pub is_premium: bool,
}

#[derive(Debug, Deserialize)]
pub struct AtlasLookupQuota {
    pub limit: i32,
    pub remaining: i32,
    pub daily_limit: i32,
    pub daily_remaining: i32,
}

pub struct AtlasClient {
    http: reqwest::Client,
    token: String,
}

impl AtlasClient {
    pub fn new() -> Result<Self> {
        let token = env::var("ATLAS_JWT_TOKEN").context("ATLAS_JWT_TOKEN not set in .env")?;
        Ok(Self {
            http: reqwest::Client::new(),
            token,
        })
    }

    pub async fn get_player(&self, steam_id: &str) -> Result<AtlasPlayerResponse> {
        let url = format!(
            "https://services.atlasrust.com/api/public/player/{}",
            steam_id
        );

        let res = self.http.get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36")
            .header("Origin", "https://atlasrust.com")
            .header("Referer", "https://atlasrust.com/")
            .header("x-access-token", &self.token)
            .send()
            .await?;

        let status = res.status();
        let text = res.text().await?;

        if !status.is_success() {
            anyhow::bail!("Atlas API returned status {}: {}", status, text);
        }

        let parsed: AtlasPlayerResponse = serde_json::from_str(&text)?;
        Ok(parsed)
    }
}
