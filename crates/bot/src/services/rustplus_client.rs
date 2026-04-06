#![allow(clippy::pedantic)]
#![allow(clippy::collapsible_if)]

use crate::Data;
use crate::db::models::{FcmCredential, GuildConfig, PairedServer, ServerChannel};
use crate::db::schema::{
    fcm_credentials::dsl as fcm_dsl, guild_configs::dsl as gc_dsl, paired_servers::dsl as ps_dsl,
    server_channels::dsl as sc_dsl,
};
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use rustplus::RustPlusClient;
use tracing::{error, info, warn};

pub async fn handle_interaction(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
    data: &Data,
) -> anyhow::Result<()> {
    let custom_id = &interaction.data.custom_id;

    if custom_id.starts_with("connect_") || custom_id.starts_with("disconnect_") {
        let parts: Vec<&str> = custom_id.split('_').collect();
        if parts.len() != 2 {
            return Ok(());
        }

        let server_id: i32 = parts[1].parse()?;
        let is_connect = parts[0] == "connect";

        // Defer interaction
        interaction.defer(&ctx.http).await?;

        if is_connect {
            if let Err(e) = connect_server(server_id, data, ctx.clone()).await {
                error!("Failed to connect to server {}: {e}", server_id);
                interaction
                    .edit_response(
                        &ctx.http,
                        serenity::EditInteractionResponse::new()
                            .content(format!("Failed to connect: {e}")),
                    )
                    .await?;
            } else {
                interaction
                    .edit_response(
                        &ctx.http,
                        serenity::EditInteractionResponse::new().content("Connecting to server..."),
                    )
                    .await?;
            }
        } else {
            if let Err(e) = disconnect_server(server_id, data, ctx.clone()).await {
                error!("Failed to disconnect from server {}: {e}", server_id);
                interaction
                    .edit_response(
                        &ctx.http,
                        serenity::EditInteractionResponse::new()
                            .content(format!("Failed to disconnect: {e}")),
                    )
                    .await?;
            } else {
                interaction
                    .edit_response(
                        &ctx.http,
                        serenity::EditInteractionResponse::new()
                            .content("Disconnected from server."),
                    )
                    .await?;
            }
        }
    }

    Ok(())
}

pub async fn boot_existing_connections(data: &Data, ctx: serenity::Context) -> anyhow::Result<()> {
    // 1. Reset all dashboards to offline initially to clear stale "Online" states
    let _ =
        crate::services::dashboard::reset_all_dashboards_offline(&ctx.http, &data.db_pool).await;

    let mut conn = data.db_pool.get()?;
    let servers: Vec<PairedServer> = ps_dsl::paired_servers
        .filter(ps_dsl::auto_reconnect.eq(1))
        .load(&mut conn)?;

    for server in servers {
        let server_id = server.id;
        let data_clone = data.clone();
        let ctx_clone = ctx.clone();
        tokio::spawn(async move {
            if let Err(e) = connect_server(server_id, &data_clone, ctx_clone).await {
                error!("Failed to auto-reconnect to server {}: {}", server_id, e);
            }
        });
    }

    Ok(())
}

pub async fn handle_discord_message(
    _ctx: &serenity::Context,
    msg: &serenity::Message,
    data: &Data,
) -> anyhow::Result<()> {
    if msg.author.bot {
        return Ok(());
    }

    let mut conn = data.db_pool.get()?;
    let channel_id_str = msg.channel_id.get().to_string();

    let server_channel: Option<ServerChannel> = sc_dsl::server_channels
        .filter(sc_dsl::chat_channel_id.eq(&channel_id_str))
        .first::<ServerChannel>(&mut conn)
        .optional()?;

    if let Some(sc) = server_channel {
        let clients = data.rustplus_clients.lock().await;
        if let Some(client) = clients.get(&sc.server_id) {
            let rust_msg = format!("[Discord] {}: {}", msg.author.name, msg.content);
            if let Err(e) = client.send_team_message(&rust_msg).await {
                error!(
                    "Failed to send team message to Rust server {}: {}",
                    sc.server_id, e
                );
            }
        }
    }

    Ok(())
}

pub async fn connect_server(
    server_id: i32,
    data: &Data,
    ctx: serenity::Context,
) -> anyhow::Result<()> {
    let mut conn = data.db_pool.get()?;

    let server: PairedServer = ps_dsl::paired_servers.find(server_id).first(&mut conn)?;

    // Set auto_reconnect to 1
    diesel::update(ps_dsl::paired_servers.find(server_id))
        .set(ps_dsl::auto_reconnect.eq(1))
        .execute(&mut conn)?;

    // Immediately update dashboard to "Connecting"
    let _ = crate::services::dashboard::update_dashboard_online(
        &ctx.http,
        &data.db_pool,
        server_id,
        None,
        None,
    )
    .await;

    let cred: FcmCredential = fcm_dsl::fcm_credentials
        .find(server.fcm_credential_id)
        .first(&mut conn)?;
    let config: GuildConfig = gc_dsl::guild_configs
        .find(&cred.guild_id)
        .first(&mut conn)?;

    let server_channel: Option<ServerChannel> = sc_dsl::server_channels
        .find(server_id)
        .first::<ServerChannel>(&mut conn)
        .optional()?;

    let mut lock = data.rustplus_clients.lock().await;

    if lock.contains_key(&server_id) {
        return Err(anyhow::anyhow!("Already connected or connecting."));
    }

    let steam_id = cred.steam_id.parse::<u64>()?;

    let mut client = RustPlusClient::new(
        server.server_ip.clone(),
        server.server_port as u16,
        steam_id,
        server.player_token,
        false, // assuming no proxy for now
    );

    client.connect().await?;

    let mut rx = match client.take_broadcast_receiver() {
        Some(r) => r,
        None => return Err(anyhow::anyhow!("Failed to acquire broadcast receiver")),
    };

    info!(
        "Connected to Rust+ server {} ({}:{})",
        server.name, server.server_ip, server.server_port
    );

    lock.insert(server_id, client.clone());

    // Fetch initial server and team info
    let server_info = match client.get_info().await {
        Ok(msg) => msg.response.and_then(|r| r.info),
        Err(e) => {
            warn!("Failed to fetch initial server info: {}", e);
            None
        }
    };

    let team_info = match client.get_team_info().await {
        Ok(msg) => msg.response.and_then(|r| r.team_info),
        Err(e) => {
            warn!("Failed to fetch initial team info: {}", e);
            None
        }
    };

    // Update Dashboard to Online
    if let Err(e) = crate::services::dashboard::update_dashboard_online(
        &ctx.http,
        &data.db_pool,
        server_id,
        server_info.as_ref(),
        team_info.as_ref(),
    )
    .await
    {
        error!("Failed to update dashboard to online: {}", e);
    }

    // Spawn a background task to listen to the receiver
    let clients_arc = data.rustplus_clients.clone();
    let pool_clone = data.db_pool.clone();
    let ctx_clone = ctx.clone();
    let server_name = server.name.clone();
    let chat_channel_id = server_channel
        .as_ref()
        .and_then(|sc| sc.chat_channel_id.clone());
    let prefix = config.in_game_prefix.clone();

    // Cache current info for updates
    let current_server_info = server_info;

    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Some(broadcast) = &msg.broadcast {
                // Handle Team Changed
                if let Some(team_changed) = &broadcast.team_changed {
                    let ti = &team_changed.team_info;
                    if let Some(si) = &current_server_info {
                        let _ = crate::services::dashboard::update_dashboard_online(
                            &ctx_clone.http,
                            &pool_clone,
                            server_id,
                            Some(si),
                            Some(ti),
                        )
                        .await;
                    }
                }

                // Handle Team Message
                if let Some(team_msg) = &broadcast.team_message {
                    let message_text = team_msg.message.message.clone();
                    let sender_name = team_msg.message.name.clone();

                    info!(
                        "[{}] Team Chat: {}: {}",
                        server_name, sender_name, message_text
                    );

                    if !message_text.starts_with(&prefix) && !message_text.starts_with("[Discord]")
                    {
                        if let Some(ref channel_id_str) = chat_channel_id {
                            if let Ok(channel_id) = channel_id_str.parse::<u64>() {
                                let channel = serenity::ChannelId::new(channel_id);
                                let discord_msg =
                                    format!("**[{}]**: {}", sender_name, message_text);
                                if let Err(e) = channel.say(&ctx_clone.http, discord_msg).await {
                                    error!("Failed to send team message to Discord: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }

        warn!("Rust+ connection to {} lost.", server_name);
        clients_arc.lock().await.remove(&server_id);

        // Update Dashboard to Offline
        let _ = crate::services::dashboard::update_dashboard_offline(
            &ctx_clone.http,
            &pool_clone,
            server_id,
        )
        .await;
    });

    Ok(())
}

pub async fn disconnect_server(
    server_id: i32,
    data: &Data,
    ctx: serenity::Context,
) -> anyhow::Result<()> {
    let mut lock = data.rustplus_clients.lock().await;

    if let Some(mut client) = lock.remove(&server_id) {
        client.disconnect();
        info!("Disconnected from server {}", server_id);
    }

    // Always reset auto_reconnect to 0
    let mut conn = data.db_pool.get()?;
    diesel::update(ps_dsl::paired_servers.find(server_id))
        .set(ps_dsl::auto_reconnect.eq(0))
        .execute(&mut conn)?;

    let _ =
        crate::services::dashboard::update_dashboard_offline(&ctx.http, &data.db_pool, server_id)
            .await;

    Ok(())
}
