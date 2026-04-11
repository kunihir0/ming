use anyhow::{Context, Result};
use rand::seq::SliceRandom;
use reqwest::Client;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, PRAGMA, USER_AGENT};
use tokio::time::{Duration, sleep};
use tracing::warn;

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0",
];

#[derive(Clone)]
pub struct SteamHttpClient {
    client: Client,
}

impl SteamHttpClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client for SteamService")?;
        Ok(Self { client })
    }

    fn get_random_user_agent() -> &'static str {
        let mut rng = rand::thread_rng();
        USER_AGENTS
            .choose(&mut rng)
            .copied()
            .unwrap_or(USER_AGENTS[0])
    }

    pub async fn fetch_html(&self, url: &str) -> Result<String> {
        let max_retries = 2;

        for attempt in 0..=max_retries {
            let request = self
                .client
                .get(url)
                .header(USER_AGENT, Self::get_random_user_agent())
                .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
                .header(
                    ACCEPT,
                    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
                )
                .header(CACHE_CONTROL, "no-cache")
                .header(PRAGMA, "no-cache");

            let response = match request.send().await {
                Ok(resp) => resp,
                Err(e) => {
                    if attempt == max_retries {
                        return Err(anyhow::anyhow!("Failed to fetch after retries: {}", e));
                    }
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let backoff = (attempt + 1) * 2;
                warn!(
                    "Rate limited (429) for {}. Retrying in {}s...",
                    url, backoff
                );
                sleep(Duration::from_secs(backoff as u64)).await;
                continue;
            }

            response
                .error_for_status_ref()
                .context("HTTP error returned by Steam")?;

            let text = response
                .text()
                .await
                .context("Failed to read response text")?;
            return Ok(text);
        }

        anyhow::bail!("Exceeded retries for {}", url)
    }
}
