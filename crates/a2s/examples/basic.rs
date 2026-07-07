use a2s::A2sClient;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <ip:port>", args[0]);
        eprintln!("Example: {} 142.232.18.23:28015", args[0]);
        std::process::exit(1);
    }

    let addr = &args[1];
    let client = A2sClient::new(5); // 5 second timeout

    println!("Querying server info from {}...", addr);
    match client.info(addr).await {
        Ok(info) => {
            println!("--- Server Info ---");
            println!("Name: {}", info.name);
            println!("Map: {}", info.map);
            println!("Game: {}", info.game);
            let cp = info.real_players.unwrap_or(info.players as u16);
            let mp = info.real_max_players.unwrap_or(info.max_players as u16);
            println!("Players: {}/{} (Bots: {})", cp, mp, info.bots);
            println!("Version: {}", info.version);
            println!("VAC Secured: {}", info.vac == 1);
        }
        Err(e) => {
            eprintln!("Failed to get server info: {}", e);
        }
    }

    println!("\nQuerying player list from {}...", addr);
    match client.players(addr).await {
        Ok(players) => {
            println!("--- Players ({}) ---", players.len());
            for p in players {
                let mins = p.duration / 60.0;
                let hours = mins / 60.0;
                if hours >= 1.0 {
                    println!(
                        "[{:02}] {} - Score: {} - Time: {:.1}h",
                        p.index, p.name, p.score, hours
                    );
                } else {
                    println!(
                        "[{:02}] {} - Score: {} - Time: {:.0}m",
                        p.index, p.name, p.score, mins
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to get player list: {}", e);
        }
    }

    Ok(())
}
