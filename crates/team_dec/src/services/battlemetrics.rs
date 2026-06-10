use reqwest::Client;
use serde::Deserialize;
use std::collections::HashSet;

use crate::error::Result;

#[derive(Debug, Deserialize)]
struct BattleMetricsAttributes {
    name: String,
}

#[derive(Debug, Deserialize)]
struct BattleMetricsIncluded {
    attributes: BattleMetricsAttributes,
}

#[derive(Debug, Deserialize)]
struct BattleMetricsResponse {
    included: Option<Vec<BattleMetricsIncluded>>,
}

pub struct BattleMetricsService {
    client: Client,
    debug: bool,
}

impl BattleMetricsService {
    pub fn new(debug: bool) -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build()
                .unwrap_or_else(|_| Client::new()),
            debug,
        }
    }

    fn log(&self, msg: &str) {
        if self.debug {
            println!("[BattleMetrics] {}", msg);
        }
    }

    pub async fn get_players(&self, server_id: &str) -> Result<HashSet<String>> {
        self.log(&format!("get_players(server_id:{})", server_id));

        let url = format!(
            "https://api.battlemetrics.com/servers/{}?include=player",
            server_id
        );

        let response = self.client.get(&url).send().await?.error_for_status()?;
        let data: BattleMetricsResponse = response.json().await?;

        let players = data
            .included
            .unwrap_or_default()
            .into_iter()
            .map(|inc| inc.attributes.name)
            .collect::<HashSet<String>>();

        self.log(&format!("get_players -> count: {}", players.len()));

        Ok(players)
    }
}
