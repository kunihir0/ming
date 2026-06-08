use rustplus::RustPlusClient;
use std::env;

#[tokio::main]
async fn main() {
    let server_ip = "45.45.239.173";
    let server_port = 28019;
    let player_token = 4; // From the log "FCM listener connected for credential 4", we don't know the exact token. Wait! 
    // We can't connect directly without the player token!
}
