use rustplus::RustPlusClient;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("debug").init();
    let _ = dotenvy::dotenv();
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let db_pool = db::establish_connection_pool(&env::var("DATABASE_URL")?);
    let mut conn = db_pool.get()?;
    use db::schema::paired_servers::dsl::*;
    use db::schema::fcm_credentials::dsl as fcm_dsl;
    use diesel::prelude::*;

    let server: db::models::PairedServer = paired_servers.first(&mut conn)?;
    let cred: db::models::FcmCredential = fcm_dsl::fcm_credentials.find(server.fcm_credential_id).first(&mut conn)?;

    let steam_id = cred.steam_id.parse::<u64>()?;
    let port = u16::try_from(server.server_port)?;

    let mut client = RustPlusClient::new(
        server.server_ip.clone(),
        port,
        steam_id,
        server.player_token,
        true, // proxy
    );

    println!("Connecting...");
    client.connect().await?;
    println!("Connected!");

    println!("Testing get_info...");
    match client.get_info().await {
        Ok(res) => println!("get_info Ok"),
        Err(e) => println!("get_info Err: {}", e),
    }

    println!("Testing get_time...");
    match client.get_time().await {
        Ok(res) => println!("get_time Ok"),
        Err(e) => println!("get_time Err: {}", e),
    }

    println!("Testing get_map...");
    match client.get_map().await {
        Ok(res) => println!("get_map Ok"),
        Err(e) => println!("get_map Err: {}", e),
    }

    println!("Testing get_map_markers...");
    match client.get_map_markers().await {
        Ok(res) => {
            let markers = res.response.unwrap().map_markers.unwrap().markers;
            println!("get_map_markers Ok. Found {} markers.", markers.len());
            let vending_markers: Vec<_> = markers.into_iter()
                .filter(|m| m.r#type == rustplus::proto::AppMarkerType::VendingMachine as i32)
                .collect();
            println!("Found {} vending markers.", vending_markers.len());
            
            for marker in vending_markers {
                for order in marker.sell_orders {
                    // Auto Turret ID is -2139580305, Scrap is -932201673
                    if order.item_id == -2139580305 || order.currency_id == -2139580305 {
                        println!("Auto Turret listing: pos ({},{}), item={}, currency={}, cost={}, qty={}",
                            marker.x, marker.y, order.item_id, order.currency_id, order.cost_per_item, order.quantity);
                    }
                }
            }
        },
        Err(e) => println!("get_map_markers Err: {}", e),
    }

    Ok(())
}
