use crate::db::DbPool;
use crate::db::models::{FcmCredential, GuildConfig, NewPairingRequest, PairedServer};
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use push_receiver::PushReceiver;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Boots up existing FCM receivers on bot startup
///
/// # Errors
/// Returns an error if the database query fails.
pub async fn boot_existing_receivers<S: std::hash::BuildHasher>(
    db_pool: &DbPool,
    ctx: serenity::Context,
    receivers: Arc<Mutex<HashMap<i32, JoinHandle<()>, S>>>,
) -> anyhow::Result<()> {
    use crate::db::schema::fcm_credentials::dsl::fcm_credentials;

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
        loop {
            info!("Starting FCM listener for credential ID {}", cred.id);

            let android_id: u64 = cred.gcm_android_id.parse().unwrap_or(0);
            let security_token: u64 = cred.gcm_security_token.parse().unwrap_or(0);

            let connection_res = PushReceiver::builder(&cred.steam_id)
                .listen(android_id, security_token)
                .await;

            let (_receiver, mut notification_stream) = match connection_res {
                Ok(res) => res,
                Err(e) => {
                    error!(
                        "Failed to connect PushReceiver for cred ID {}: {:?}. Retrying in 5s...",
                        cred.id, e
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };

            info!("FCM listener connected for credential ID {}", cred.id);

            while let Some(notif) = notification_stream.recv().await {
                if let Err(e) = handle_fcm_message(&notif, &cred, &db_pool, &ctx).await {
                    error!("Error handling FCM message: {e}");
                }
            }

            warn!("FCM listener stream ended for credential ID {}. Reconnecting...", cred.id);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    })
}

#[allow(clippy::collapsible_if)]
#[allow(clippy::too_many_lines)]
async fn handle_fcm_message(
    payload: &push_receiver::Notification,
    cred: &FcmCredential,
    db_pool: &DbPool,
    ctx: &serenity::Context,
) -> anyhow::Result<()> {
    use crate::db::schema::guild_configs::dsl as gc_dsl;
    use crate::db::schema::paired_servers::dsl as ps_dsl;
    use crate::db::schema::pairing_requests::dsl as pr_dsl;

    let app_data = &payload.app_data;

    let channel_id = app_data
        .iter()
        .find(|item| item.key == "channelId")
        .map(|item| item.value.as_str());

    info!(
        "Received FCM notification: channelId={:?}, total_items={}, data={:?}",
        channel_id,
        app_data.len(),
        app_data
    );

    // SUPER-FUZZY DETECTION: Scan every single field for anything that looks like a pairing JSON
    for item in app_data {
        let val = item.value.trim();
        if !val.starts_with('{') {
            continue;
        }

        let Ok(body) = serde_json::from_str::<Value>(val) else {
            continue;
        };

        // If it has an IP and a playerToken, it's a pairing request regardless of 'type'
        if body.get("ip").is_some() && body.get("playerToken").is_some() {
            let ip = body.get("ip").and_then(Value::as_str).unwrap_or("");
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

            // Extract name from the body or notification title
            let title = body
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| {
                    app_data
                        .iter()
                        .find(|i| i.key == "title" || i.key == "gcm.notification.title")
                        .map(|i| i.value.as_str())
                })
                .unwrap_or("Unknown Server");

            if port == 0 || token == 0 || ip.is_empty() {
                continue;
            }

            info!("Caught Pairing Request (Fuzzy): {title} ({ip}:{port})");

            // Save to DB
            let mut conn = db_pool.get()?;

            // Check if already paired
            let existing: Option<PairedServer> = ps_dsl::paired_servers
                .filter(ps_dsl::server_ip.eq(ip))
                .filter(ps_dsl::server_port.eq(port))
                .filter(ps_dsl::player_token.eq(token))
                .first::<PairedServer>(&mut conn)
                .optional()?;

            if existing.is_some() {
                info!("Server already paired, skipping.");
                return Ok(());
            }

            // Get Management Channel
            let config: GuildConfig = gc_dsl::guild_configs
                .filter(gc_dsl::guild_id.eq(&cred.guild_id))
                .first(&mut conn)?;

            let Some(m_chan_id_str) = config.management_channel_id else {
                error!("No management channel set for guild {}", cred.guild_id);
                return Ok(());
            };

            let m_chan_id = serenity::ChannelId::new(m_chan_id_str.parse::<u64>()?);

            // Create Pairing Request
            let request_id = Uuid::new_v4().to_string();
            let new_request = NewPairingRequest {
                id: request_id.clone(),
                guild_id: cred.guild_id.clone(),
                fcm_credential_id: cred.id,
                server_ip: ip.to_string(),
                server_port: port,
                player_token: token,
                name: title.to_string(),
            };

            diesel::insert_into(pr_dsl::pairing_requests)
                .values(&new_request)
                .execute(&mut conn)?;

            // Post Approval Message
            let embed = serenity::CreateEmbed::new()
                .title("New Pairing Request")
                .color(0x00FF_FF00) // Yellow
                .description(format!(
                    "A new Rust server pairing request has been received.\n\n**Server:** {title}\n**IP:** `{ip}:{port}`"
                ))
                .footer(serenity::CreateEmbedFooter::new(format!("Request ID: {request_id}")));

            let approve_btn = serenity::CreateButton::new(format!("pair_approve_{request_id}"))
                .label("Approve")
                .style(serenity::ButtonStyle::Success);

            let ignore_btn = serenity::CreateButton::new(format!("pair_ignore_{request_id}"))
                .label("Ignore")
                .style(serenity::ButtonStyle::Secondary);

            let components = serenity::CreateActionRow::Buttons(vec![approve_btn, ignore_btn]);

            m_chan_id
                .send_message(
                    &ctx.http,
                    serenity::CreateMessage::new()
                        .embed(embed)
                        .components(vec![components]),
                )
                .await?;

            return Ok(()); // Stop after finding the first valid pairing JSON
        }
    }

    Ok(())
}
