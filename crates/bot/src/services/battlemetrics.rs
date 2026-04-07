use reqwest::Client;
use serde::Deserialize;
use tracing::error;

#[derive(Debug, Deserialize)]
pub struct BattlemetricsServerResponse {
    pub data: Vec<BattlemetricsServerData>,
}

#[derive(Debug, Deserialize)]
pub struct BattlemetricsServerData {
    pub id: String,
    pub attributes: BattlemetricsAttributes,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattlemetricsAttributes {
    pub name: String,
    pub players: i32,
    pub max_players: i32,
    pub details: Option<BattlemetricsDetails>,
}

#[derive(Debug, Deserialize)]
pub struct BattlemetricsDetails {
    pub rust_queued_players: Option<i32>,
}

pub struct BattlemetricsService {
    client: Client,
}

impl BattlemetricsService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Fetches server data from Battlemetrics by address.
    ///
    /// # Errors
    /// Returns an error if the network request fails or JSON parsing fails.
    pub async fn get_server_by_address(
        &self,
        ip: &str,
        _port: i32,
    ) -> anyhow::Result<Option<BattlemetricsServerData>> {
        // Battlemetrics search is fuzzy. Searching by IP is generally reliable.
        // We avoid searching by Port because we only have the Rust+ App Port,
        // but Battlemetrics indexes by the Game Port.
        let url = format!(
            "https://api.battlemetrics.com/servers?filter[search]={ip}&filter[game]=rust"
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            error!("Battlemetrics API error: {} for URL: {}", resp.status(), url);
            return Ok(None);
        }

        let data: BattlemetricsServerResponse = resp.json().await?;
        
        // Return the first server found.
        Ok(data.data.into_iter().next())
    }

    /// Fetches server population string.
    ///
    /// # Errors
    /// Returns an error if the server is not found or API fails.
    pub async fn get_server_pop(&self, ip: &str, port: i32) -> anyhow::Result<String> {
        let server = self.get_server_by_address(ip, port).await?;

        match server {
            Some(s) => {
                let players = s.attributes.players;
                let max = s.attributes.max_players;
                let queue = s
                    .attributes
                    .details
                    .and_then(|d| d.rust_queued_players)
                    .unwrap_or(0);

                if queue > 0 {
                    Ok(format!("{players}/{max} ({queue})"))
                } else {
                    Ok(format!("{players}/{max}"))
                }
            }
            None => Err(anyhow::anyhow!("Server not found on Battlemetrics")),
        }
    }
}

impl Default for BattlemetricsService {
    fn default() -> Self {
        Self::new()
    }
}
