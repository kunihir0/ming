use crate::db::DbPool;
use crate::db::models::{FcmCredential, NewPairedServer, PairedServer};
use crate::db::schema::fcm_credentials::dsl::fcm_credentials;
use crate::db::schema::paired_servers::dsl::paired_servers;
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use push_receiver::PushReceiver;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Boots up existing FCM receivers on bot startup
///
/// # Errors
/// Returns an error if the database query fails.
pub async fn boot_existing_receivers<S: std::hash::BuildHasher>(
    db_pool: &DbPool,
    ctx: serenity::Context,
    receivers: Arc<Mutex<HashMap<i32, JoinHandle<()>, S>>>,
) -> anyhow::Result<()> {
    let mut conn = db_pool.get()?;
    let creds = fcm_credentials.load::<FcmCredential>(&mut conn)?;

    let mut lock = receivers.lock().await;
    for cred in creds {
        let handle = start_listener(cred.clone(), db_pool.clone(), ctx.clone());
        lock.insert(cred.id, handle);
    }

    info!("Booted existing FCM receivers");
    Ok(())
}

#[must_use]
pub fn start_listener(
    cred: FcmCredential,
    db_pool: DbPool,
    ctx: serenity::Context,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // FCM connect requires sending sender_id (Steam ID usually, or hardcoded project ID)
        // Using Steam ID for authorized_entity.
        info!("Starting FCM listener for credential ID {}", cred.id);

        // The user provided their own Android ID and Security Token via the slash command,
        // so we don't need to re-register with Google (which is failing with 404 anyway).
        // We just start the MCS socket listener directly.
        let android_id: u64 = cred.gcm_android_id.parse().unwrap_or(0);
        let security_token: u64 = cred.gcm_security_token.parse().unwrap_or(0);

        let connection_res = PushReceiver::builder(&cred.steam_id)
            .listen(android_id, security_token)
            .await;

        let (_receiver, mut notification_stream) = match connection_res {
            Ok(res) => res,
            Err(e) => {
                error!(
                    "Failed to connect PushReceiver for cred ID {}: {:?}",
                    cred.id, e
                );
                return;
            }
        };

        info!("FCM listener connected for credential ID {}", cred.id);

        while let Some(notif) = notification_stream.recv().await {
            handle_fcm_message(&notif, &cred, &db_pool, &ctx);
        }

        warn!("FCM listener stream ended for credential ID {}", cred.id);
    })
}

#[allow(clippy::collapsible_if)]
#[allow(clippy::too_many_lines)]
fn handle_fcm_message(
    payload: &push_receiver::Notification,
    cred: &FcmCredential,
    db_pool: &DbPool,
    ctx: &serenity::Context,
) {
    info!(
        "Received FCM payload with {} app_data items",
        payload.app_data.len()
    );

    let app_data = &payload.app_data;

    let channel_id = app_data
        .iter()
        .find(|item| item.key == "channelId")
        .map(|item| item.value.as_str());

    if channel_id != Some("pairing") {
        return;
    }

    let body_str = app_data
        .iter()
        .find(|item| item.key == "body")
        .map(|item| item.value.as_str());

    if let Some(body_str) = body_str {
        let Ok(body) = serde_json::from_str::<Value>(body_str) else {
            warn!("Failed to parse pairing body JSON: {}", body_str);
            return;
        };

        if body.get("type").and_then(Value::as_str) == Some("server") {
            let ip = body.get("ip").and_then(Value::as_str).unwrap_or("");
            // Rust+ appPort is sometimes string, sometimes number depending on the JSON
            let port = body
                .get("port")
                .and_then(|v| {
                    if let Some(s) = v.as_str() {
                        s.parse::<i32>().ok()
                    } else {
                        #[allow(clippy::cast_possible_truncation)]
                        v.as_i64().map(|n| n as i32)
                    }
                })
                .unwrap_or(0);
            let token = body
                .get("playerToken")
                .and_then(|v| {
                    if let Some(s) = v.as_str() {
                        s.parse::<i32>().ok()
                    } else {
                        #[allow(clippy::cast_possible_truncation)]
                        v.as_i64().map(|n| n as i32)
                    }
                })
                .unwrap_or(0);

            // Name is typically extracted from the title of the pairing notification, or desc.
            // Let's check title first.
            let title = app_data
                .iter()
                .find(|item| item.key == "title")
                .map_or("Unknown Server", |item| item.value.as_str());

            if port == 0 || token == 0 || ip.is_empty() {
                warn!("Received invalid pairing payload body: {:?}", body);
                return;
            }

            info!("Pairing new server: {} ({}:{})", title, ip, port);

            // Save to DB
            let mut conn = match db_pool.get() {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to get DB conn: {e}");
                    return;
                }
            };

            let new_server = NewPairedServer {
                fcm_credential_id: cred.id,
                server_ip: ip.to_string(),
                server_port: port,
                player_token: token,
                name: title.to_string(),
            };

            let insert_res = diesel::insert_into(paired_servers)
                .values(&new_server)
                .execute(&mut conn);

            if let Err(e) = insert_res {
                let err_str = e.to_string();
                if err_str.contains("UNIQUE constraint failed") {
                    info!("Server already paired, skipping dashboard creation.");
                    return;
                }
                error!("Failed to save paired server: {e}");
                return;
            }

            // Get the inserted server to pass to the dashboard handler
            let inserted_server = match paired_servers
                .filter(crate::db::schema::paired_servers::dsl::server_ip.eq(&new_server.server_ip))
                .filter(
                    crate::db::schema::paired_servers::dsl::server_port.eq(&new_server.server_port),
                )
                .filter(
                    crate::db::schema::paired_servers::dsl::player_token
                        .eq(&new_server.player_token),
                )
                .first::<PairedServer>(&mut conn)
            {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to fetch the newly inserted paired server: {e}");
                    return;
                }
            };

            // Setup channels and dashboard
            tokio::spawn({
                let db_pool = db_pool.clone();
                let ctx = ctx.clone();
                let guild_id = cred.guild_id.clone();
                async move {
                    if let Err(e) = crate::services::dashboard::handle_new_paired_server(
                        &db_pool,
                        &ctx,
                        &guild_id,
                        &inserted_server,
                    )
                    .await
                    {
                        error!("Failed to setup dashboard for new server: {e}");
                    }
                }
            });
        }
    }
}
