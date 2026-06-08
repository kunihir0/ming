use anyhow::Result;
use db::models::{FcmCredential, NewPairedServer, PairedServer};
use db::DbPool;
use push_receiver::PushReceiver;
use serde_json::Value;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::connection_manager::ConnectionManager;

/// Starts FCM listeners for all credentials in the database.
/// Auto-pairs and auto-connects any new server that sends a pairing notification.
pub async fn boot_fcm_listeners(
    db_pool: &DbPool,
    conn_mgr: Arc<ConnectionManager>,
) -> Result<Vec<JoinHandle<()>>> {
    let mut conn = db_pool.get()?;
    use db::schema::fcm_credentials::dsl::*;
    use diesel::prelude::*;

    let creds = fcm_credentials.load::<FcmCredential>(&mut conn)?;
    let mut handles = Vec::new();

    for cred in creds {
        let handle = spawn_fcm_listener(cred, db_pool.clone(), conn_mgr.clone());
        handles.push(handle);
    }

    info!("Booted {} FCM listeners", handles.len());
    Ok(handles)
}

/// Spawns an FCM listener for a single credential.
/// Used both at boot and when a new credential is added via `/credentials add`.
pub fn spawn_single_listener(
    cred: FcmCredential,
    db_pool: DbPool,
    conn_mgr: Arc<ConnectionManager>,
) -> JoinHandle<()> {
    spawn_fcm_listener(cred, db_pool, conn_mgr)
}

fn spawn_fcm_listener(
    cred: FcmCredential,
    db_pool: DbPool,
    conn_mgr: Arc<ConnectionManager>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            info!("Starting FCM listener for credential {}", cred.id);

            let android_id: u64 = cred.gcm_android_id.parse().unwrap_or(0);
            let security_token: u64 = cred.gcm_security_token.parse().unwrap_or(0);

            let connection_res = PushReceiver::builder(&cred.steam_id)
                .listen(android_id, security_token)
                .await;

            let (_receiver, mut notification_stream) = match connection_res {
                Ok(res) => res,
                Err(e) => {
                    error!(
                        "FCM connect failed for cred {}: {:?}. Retrying in 5s...",
                        cred.id, e
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };

            info!("FCM listener connected for credential {}", cred.id);

            while let Some(notif) = notification_stream.recv().await {
                if let Err(e) =
                    handle_fcm_notification(&notif, &cred, &db_pool, &conn_mgr).await
                {
                    error!("Error handling FCM notification: {}", e);
                }
            }

            warn!("FCM stream ended for credential {}. Reconnecting...", cred.id);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    })
}

async fn handle_fcm_notification(
    payload: &push_receiver::Notification,
    cred: &FcmCredential,
    db_pool: &DbPool,
    conn_mgr: &ConnectionManager,
) -> Result<()> {
    use db::schema::paired_servers::dsl as ps_dsl;
    use diesel::prelude::*;

    for item in &payload.app_data {
        let val = item.value.trim();
        if !val.starts_with('{') {
            continue;
        }

        let Ok(body) = serde_json::from_str::<Value>(val) else {
            continue;
        };

        // Detect pairing payload: must have ip + playerToken
        if body.get("ip").is_none() || body.get("playerToken").is_none() {
            continue;
        }

        let ip = body.get("ip").and_then(Value::as_str).unwrap_or("");
        let port = parse_i32_field(&body, "port").unwrap_or(0);
        let token = parse_i32_field(&body, "playerToken").unwrap_or(0);
        let name = body
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Unknown Server");

        if port == 0 || token == 0 || ip.is_empty() {
            continue;
        }

        info!("Detected pairing: {} ({}:{})", name, ip, port);

        let mut conn = db_pool.get()?;

        // Skip if already paired with these exact credentials
        let existing: Option<PairedServer> = ps_dsl::paired_servers
            .filter(ps_dsl::server_ip.eq(ip))
            .filter(ps_dsl::server_port.eq(port))
            .filter(ps_dsl::player_token.eq(token))
            .first::<PairedServer>(&mut conn)
            .optional()?;

        if let Some(srv) = existing {
            info!("Server {} already paired (id={}), ensuring it is connected", name, srv.id);
            
            // Re-enable auto_reconnect in case it was disabled
            diesel::update(ps_dsl::paired_servers.find(srv.id))
                .set(ps_dsl::auto_reconnect.eq(1))
                .execute(&mut conn)?;

            // Ensure it's connected
            let is_connected = {
                let lock = conn_mgr.clients.lock().await;
                lock.contains_key(&srv.id)
            };

            if !is_connected {
                if let Err(e) = conn_mgr.connect(srv.id).await {
                    error!("Failed to auto-connect {}: {}", name, e);
                }
            }

            return Ok(());
        }

        // Auto-pair: insert directly into paired_servers
        let new_server = NewPairedServer {
            fcm_credential_id: cred.id,
            server_ip: ip.to_string(),
            server_port: port,
            player_token: token,
            name: name.to_string(),
            auto_reconnect: 1,
        };

        diesel::insert_into(ps_dsl::paired_servers)
            .values(&new_server)
            .execute(&mut conn)?;

        // Fetch back to get the assigned id
        let inserted: PairedServer = ps_dsl::paired_servers
            .filter(ps_dsl::server_ip.eq(ip))
            .filter(ps_dsl::server_port.eq(port))
            .filter(ps_dsl::player_token.eq(token))
            .first::<PairedServer>(&mut conn)?;

        info!("Auto-paired {} as server id {}", name, inserted.id);

        // Auto-connect
        if let Err(e) = conn_mgr.connect(inserted.id).await {
            error!("Auto-connect failed for {}: {}", name, e);
        }

        return Ok(());
    }

    Ok(())
}

fn parse_i32_field(body: &Value, key: &str) -> Option<i32> {
    body.get(key).and_then(|v| {
        if let Some(s) = v.as_str() {
            s.parse::<i32>().ok()
        } else {
            #[allow(clippy::cast_possible_truncation)]
            v.as_i64().map(|n| n as i32)
        }
    })
}
