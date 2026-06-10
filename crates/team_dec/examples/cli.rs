use team_dec::TeamDetectorBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Test equivalent to:
    // npm start -- -b 26980986 -s 76561198142964864 76561198160928230 ... -r 2

    let detector = TeamDetectorBuilder::new()
        .recursive_depth(2)
        .max_profiles(200)
        .include_offline(false)
        .debug(true)
        .build();

    let server_id = "26980986";
    let seeds = vec![
        "76561198142964864".to_string(),
        "76561198160928230".to_string(),
        "76561198014276793".to_string(),
        "76561198287503770".to_string(),
        "76561198370541782".to_string(),
        "76561198992641949".to_string(),
        "76561198060312433".to_string(),
        "76561198110012505".to_string(),
    ];

    println!("Starting search on server {}...", server_id);
    let (found_players, graph) = detector.run(server_id, seeds).await?;

    println!("\nTeam Detector Result:\n");
    println!(
        "{:<34}{:<19}{:<25}{:<12}{}",
        "Name:", "SteamID:", "Steam Status:", "BM Status:", "Link:"
    );

    for player in found_players {
        let steam_id = player.steam_id.unwrap_or_else(|| "N/A".to_string());
        let status = player.status.unwrap_or_else(|| "N/A".to_string());
        let is_on_server = if player.is_on_server.unwrap_or(false) {
            "Online"
        } else {
            "Offline"
        };
        let link = if steam_id != "N/A" {
            format!(
                "https://steamcommunity.com/profiles/{}/?l=english",
                steam_id
            )
        } else {
            "".to_string()
        };

        println!(
            "{:<34}{:<19}{:<25}{:<12}{}",
            player.name, steam_id, status, is_on_server, link
        );
    }

    println!(
        "\nGraph Output: {} nodes, {} edges",
        graph.nodes.len(),
        graph.edges.len()
    );

    Ok(())
}
