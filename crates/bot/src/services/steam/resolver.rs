use crate::services::steam::client::SteamHttpClient;
use anyhow::{Context, Result};
use moka::future::Cache;
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};
use std::time::Duration;

static REGEX_STEAMID64: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[0-9]{17}$").expect("valid regex"));
static REGEX_PROFILE_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"/profiles/([0-9]{17})").expect("valid regex"));
static REGEX_VANITY_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"/id/([^/]+)").expect("valid regex"));

pub struct SteamIdResolver {
    client: SteamHttpClient,
    vanity_cache: Cache<String, String>,
}

impl SteamIdResolver {
    pub fn new(client: SteamHttpClient) -> Self {
        Self {
            client,
            vanity_cache: Cache::builder()
                .time_to_live(Duration::from_secs(60 * 60)) // 1 hour cache for vanity resolution
                .build(),
        }
    }

    fn is_steam_id64(id: &str) -> bool {
        REGEX_STEAMID64.is_match(id)
    }

    pub async fn resolve_to_id64(&self, input: &str) -> Result<String> {
        if Self::is_steam_id64(input) {
            return Ok(input.to_string());
        }

        if let Some(caps) = REGEX_PROFILE_URL.captures(input) {
            if let Some(id) = caps.get(1) {
                return Ok(id.as_str().to_string());
            }
        }

        let vanity_name = if let Some(caps) = REGEX_VANITY_URL.captures(input) {
            caps.get(1).map(|m| m.as_str()).unwrap_or(input)
        } else {
            input
        };

        if let Some(cached_id) = self.vanity_cache.get(vanity_name).await {
            return Ok(cached_id);
        }

        // Fetch XML representation
        let url = format!("https://steamcommunity.com/id/{}/?xml=1", vanity_name);
        let xml = self
            .client
            .fetch_html(&url)
            .await
            .context("Failed to fetch XML for vanity resolution")?;

        let steam_id64 = {
            let document = Html::parse_document(&xml);
            let id_sel = Selector::parse("steamID64").expect("valid selector"); // Works well enough on XML if we pretend it's HTML

            document
                .select(&id_sel)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
        };

        if let Some(id) = steam_id64 {
            if Self::is_steam_id64(&id) {
                self.vanity_cache
                    .insert(vanity_name.to_string(), id.clone())
                    .await;
                return Ok(id);
            }
        }

        anyhow::bail!("Could not resolve vanity ID '{}' to SteamID64", vanity_name)
    }
}
