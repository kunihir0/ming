# team_dec_rs

A Rust library crate for detecting team associations and player connections across Steam and BattleMetrics. Designed to be lightweight, type-safe, and easily integrable into a Discord Bot.

## Features

- **Concurrent Async Design:** Uses `tokio` for efficient asynchronous HTTP requests.
- **Steam Community Scraper:** Scrapes public friend lists and Steam profiles using `reqwest` and `scraper`.
- **steamid.com Integration:** Connects to `steamid.com` using a custom Astro JSON decoder to find historically recorded or hidden friends.
- **BattleMetrics Verification:** Cross-references found Steam players against an active BattleMetrics server player list via the REST API.
- **Robust Rate Limiting:** Enforces strict delays (e.g. 4 seconds for Steam) to prevent IP bans.
- **Builder Pattern API:** Easy configuration of search depth, max profiles, and targets.

## Usage Example

```rust
use team_dec_rs::TeamDetectorBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let detector = TeamDetectorBuilder::new()
        .recursive_depth(2)
        .max_profiles(200)
        .include_offline(false)
        .build();

    let server_id = "26980986";
    let seeds = vec!["76561198142964864".to_string()];

    // `found_players` contains the target nodes. 
    // `graph_data` contains all nodes and edges (connections).
    let (found_players, graph_data) = detector.run(server_id, seeds).await?;

    for player in found_players {
        println!("{} is {}", player.name, if player.is_on_server.unwrap_or(false) { "Online" } else { "Offline" });
    }

    Ok(())
}
```

## Running the Example

```bash
cargo run --example cli
```
