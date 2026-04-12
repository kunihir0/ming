pub mod client;
pub mod parser;
pub mod resolver;
pub mod types;

use crate::services::steam::client::SteamHttpClient;
use crate::services::steam::resolver::SteamIdResolver;
use crate::services::steam::types::{SteamFriend, SteamProfile};
use anyhow::{Context, Result};
use moka::future::Cache;
use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

static REGEX_VANITY_EXTRACT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"steamcommunity\.com/id/([^/&?]+)").expect("valid regex"));

pub struct SteamService {
    client: SteamHttpClient,
    resolver: SteamIdResolver,
    profile_cache: Cache<String, SteamProfile>,
    friends_cache: Cache<String, Vec<SteamFriend>>,
}

impl SteamService {
    /// Create a new `SteamService`
    ///
    /// # Errors
    /// Returns an error if the HTTP client fails to initialize.
    pub fn new() -> Result<Self> {
        let client = SteamHttpClient::new()?;
        let resolver = SteamIdResolver::new(client.clone());

        let profile_cache = Cache::builder()
            .time_to_live(Duration::from_secs(5 * 60))
            .build();

        let friends_cache = Cache::builder()
            .time_to_live(Duration::from_secs(5 * 60))
            .build();

        Ok(Self {
            client,
            resolver,
            profile_cache,
            friends_cache,
        })
    }

    /// Fetch Steam profile by ID or vanity URL
    ///
    /// # Errors
    /// Returns an error if profile cannot be fetched or parsed.
    #[allow(clippy::collapsible_if)]
    pub async fn get_profile(&self, input: &str) -> Result<SteamProfile> {
        let steam_id64 = self.resolver.resolve_to_id64(input).await?;

        if let Some(cached) = self.profile_cache.get(&steam_id64).await {
            return Ok(cached);
        }

        let url = format!("https://steamcommunity.com/profiles/{steam_id64}/?l=english");
        let html = self.client.fetch_html(&url).await?;

        let mut profile = parser::parse_profile(&html, steam_id64.clone())
            .context("Failed to parse Steam profile")?;

        if profile.vanity_id.is_none() {
            if let Some(caps) = REGEX_VANITY_EXTRACT.captures(&html) {
                if let Some(vanity) = caps.get(1) {
                    profile.vanity_id = Some(vanity.as_str().to_string());
                }
            }
        }

        self.profile_cache.insert(steam_id64, profile.clone()).await;
        Ok(profile)
    }

    /// Fetch Steam friends by ID or vanity URL
    ///
    /// # Errors
    /// Returns an error if friends cannot be fetched or parsed.
    pub async fn get_friends(&self, input: &str) -> Result<Vec<SteamFriend>> {
        let steam_id64 = self.resolver.resolve_to_id64(input).await?;

        if let Some(cached) = self.friends_cache.get(&steam_id64).await {
            return Ok(cached);
        }

        let url = format!("https://steamcommunity.com/profiles/{steam_id64}/friends/?l=english");
        let html = match self.client.fetch_html(&url).await {
            Ok(h) => h,
            Err(e) => {
                // If it fails, it might be due to a private profile. We return empty list.
                tracing::debug!(
                    "Failed to fetch friends for {} (might be private): {}",
                    steam_id64,
                    e
                );
                return Ok(Vec::new());
            }
        };

        let friends = parser::parse_friends(&html).context("Failed to parse Steam friends")?;

        self.friends_cache.insert(steam_id64, friends.clone()).await;
        Ok(friends)
    }

    /// Fetch mutual friends between two players
    ///
    /// # Errors
    /// Returns an error if fetching either player's friends fails.
    pub async fn get_mutual_friends(
        &self,
        player_a: &str,
        player_b: &str,
    ) -> Result<Vec<SteamFriend>> {
        let (friends_a, friends_b) =
            tokio::try_join!(self.get_friends(player_a), self.get_friends(player_b))?;

        let mut mutuals = Vec::new();
        for friend in friends_a {
            if friends_b.iter().any(|f| f.steam_id64 == friend.steam_id64) {
                mutuals.push(friend);
            }
        }

        Ok(mutuals)
    }

    /// Resolve an input to a `SteamID64`
    ///
    /// # Errors
    /// Returns an error if the ID cannot be resolved.
    pub async fn resolve_id(&self, input: &str) -> Result<String> {
        self.resolver.resolve_to_id64(input).await
    }
}
