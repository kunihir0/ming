use anyhow::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let url = "https://api.battlemetrics.com/servers/26980986?include=player";
    let resp = reqwest::get(url).await?.json::<Value>().await?;
    
    if let Some(included) = resp.get("included").and_then(|v| v.as_array()) {
        println!("Found {} included items", included.len());
        for item in included.iter().take(5) {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if item_type == "player" {
                let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = item.pointer("/attributes/name").and_then(|v| v.as_str()).unwrap_or("");
                println!("Player: {} ({})", name, id);
            }
        }
    }
    
    Ok(())
}
