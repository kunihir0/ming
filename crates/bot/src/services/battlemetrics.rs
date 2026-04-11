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

#[derive(Debug, Deserialize)]
pub struct BattlemetricsPlayerResponse {
    pub included: Option<Vec<BattlemetricsIncluded>>,
}

#[derive(Debug, Deserialize)]
pub struct BattlemetricsIncluded {
    #[serde(rename = "type")]
    pub item_type: String,
    pub attributes: Option<BattlemetricsIncludedAttributes>,
}

#[derive(Debug, Deserialize)]
pub struct BattlemetricsIncludedAttributes {
    pub name: String,
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

    /// Fetches server data from Battlemetrics by ID.
    ///
    /// # Errors
    /// Returns an error if the network request fails or JSON parsing fails.
    pub async fn get_server_by_id(
        &self,
        server_id: &str,
    ) -> anyhow::Result<Option<BattlemetricsServerData>> {
        let url = format!("https://api.battlemetrics.com/servers/{}", server_id);

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(None);
        }

        #[derive(Debug, Deserialize)]
        struct SingleServerResponse {
            data: BattlemetricsServerData,
        }

        let data: SingleServerResponse = resp.json().await?;
        Ok(Some(data.data))
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
        let url =
            format!("https://api.battlemetrics.com/servers?filter[search]={ip}&filter[game]=rust");

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            error!(
                "Battlemetrics API error: {} for URL: {}",
                resp.status(),
                url
            );
            return Ok(None);
        }

        let data: BattlemetricsServerResponse = resp.json().await?;

        // Return the first server found.
        Ok(data.data.into_iter().next())
    }

    /// Fetches active player names for a specific server by ID.
    ///
    /// # Errors
    /// Returns an error if the server is not found or API fails.
    pub async fn get_active_players(&self, server_id: &str) -> anyhow::Result<Vec<String>> {
        let url = format!(
            "https://api.battlemetrics.com/servers/{}?include=player",
            server_id
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!(
                "Battlemetrics API error: {}",
                resp.status()
            ));
        }

        let data: BattlemetricsPlayerResponse = resp.json().await?;
        let mut players = Vec::new();

        if let Some(included) = data.included {
            for item in included {
                if item.item_type == "player" {
                    if let Some(attrs) = item.attributes {
                        players.push(attrs.name);
                    }
                }
            }
        }

        Ok(players)
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
                let queue = match s.attributes.details.and_then(|d| d.rust_queued_players) {
                    Some(q) => q,
                    None => 0,
                };

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
