#![allow(clippy::pedantic)]
#![allow(clippy::collapsible_if)]

use crate::db::models::{FcmCredential, PairedServer, ServerChannel};
use crate::db::schema::{fcm_credentials::dsl as fcm_dsl, paired_servers::dsl as ps_dsl, server_channels::dsl as sc_dsl};
use crate::Data;
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
                interaction.edit_response(&ctx.http, serenity::EditInteractionResponse::new().content(format!("Failed to connect: {e}"))).await?;
            } else {
                interaction.edit_response(&ctx.http, serenity::EditInteractionResponse::new().content("Connecting to server...")).await?;
            }
        } else {
            if let Err(e) = disconnect_server(server_id, data, ctx.clone()).await {
                error!("Failed to disconnect from server {}: {e}", server_id);
                interaction.edit_response(&ctx.http, serenity::EditInteractionResponse::new().content(format!("Failed to disconnect: {e}"))).await?;
            } else {
                interaction.edit_response(&ctx.http, serenity::EditInteractionResponse::new().content("Disconnected from server.")).await?;
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
    let cred: FcmCredential = fcm_dsl::fcm_credentials.find(server.fcm_credential_id).first(&mut conn)?;

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

    info!("Connected to Rust+ server {} ({}:{})", server.name, server.server_ip, server.server_port);

    lock.insert(server_id, client.clone());

    // Update Dashboard to Online
    if let Ok(server_channel) = sc_dsl::server_channels.find(server_id).first::<ServerChannel>(&mut conn) {
        if let (Some(channel_id_str), Some(msg_id_str)) = (server_channel.dashboard_channel_id, server_channel.dashboard_message_id) {
            if let (Ok(channel_id), Ok(msg_id)) = (channel_id_str.parse::<u64>(), msg_id_str.parse::<u64>()) {
                let channel = serenity::ChannelId::new(channel_id);
                let message_id = serenity::MessageId::new(msg_id);
                
                let embed = serenity::CreateEmbed::new()
                    .title(format!("{} Dashboard", server.name))
                    .color(0x0000_FF00) // Green
                    .description("Status: 🟢 **Online**")
                    .field("Server IP", &server.server_ip, true)
                    .field("Port", server.server_port.to_string(), true)
                    .footer(serenity::CreateEmbedFooter::new(format!("client.connect {}:{}", server.server_ip, server.server_port)));

                let _ = channel.edit_message(&ctx.http, message_id, serenity::EditMessage::new().embed(embed)).await;
            }
        }
    }

    // Spawn a background task to listen to the receiver
    let clients_arc = data.rustplus_clients.clone();
    let pool_clone = data.db_pool.clone();
    let ctx_clone = ctx.clone();
    let server_name = server.name.clone();

    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            // Handle AppTeamMessage, AppEntityChanged, etc
            if let Some(team_msg) = msg.broadcast.as_ref().and_then(|b| b.team_message.as_ref()) {
                let message_text = team_msg.message.message.clone();
                // We'd parse in_game_prefix here and route back or post to discord chat_channel_id
                info!("[{}] Team Chat: {}: {}", server_name, team_msg.message.name, message_text);
            }
        }
        
        warn!("Rust+ connection to {} lost.", server_name);
        clients_arc.lock().await.remove(&server_id);

        // Update Dashboard to Offline
        if let Ok(mut conn) = pool_clone.get() {
            if let Ok(server_channel) = sc_dsl::server_channels.find(server_id).first::<ServerChannel>(&mut conn) {
                if let (Some(channel_id_str), Some(msg_id_str)) = (server_channel.dashboard_channel_id, server_channel.dashboard_message_id) {
                    if let (Ok(channel_id), Ok(msg_id)) = (channel_id_str.parse::<u64>(), msg_id_str.parse::<u64>()) {
                        let channel = serenity::ChannelId::new(channel_id);
                        let message_id = serenity::MessageId::new(msg_id);
                        
                        let embed = serenity::CreateEmbed::new()
                            .title(format!("{} Dashboard", server_name))
                            .color(0x00FF_0000) // Red
                            .description("Status: 🔴 **Offline**")
                            .field("Server IP", "???", true)
                            .field("Port", "???", true);

                        let _ = channel.edit_message(&ctx_clone.http, message_id, serenity::EditMessage::new().embed(embed)).await;
                    }
                }
            }
        }
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

        // Fetch server name from DB
        let mut conn = data.db_pool.get()?;
        if let Ok(server) = ps_dsl::paired_servers.find(server_id).first::<PairedServer>(&mut conn) {
            // Update Dashboard to Offline
            if let Ok(server_channel) = sc_dsl::server_channels.find(server_id).first::<ServerChannel>(&mut conn) {
                if let (Some(channel_id_str), Some(msg_id_str)) = (server_channel.dashboard_channel_id, server_channel.dashboard_message_id) {
                    if let (Ok(channel_id), Ok(msg_id)) = (channel_id_str.parse::<u64>(), msg_id_str.parse::<u64>()) {
                        let channel = serenity::ChannelId::new(channel_id);
                        let message_id = serenity::MessageId::new(msg_id);
                        
                        let embed = serenity::CreateEmbed::new()
                            .title(format!("{} Dashboard", server.name))
                            .color(0x00FF_0000) // Red
                            .description("Status: 🔴 **Offline**\n*Click Connect to start the Rust+ bridge.*")
                            .field("Server IP", &server.server_ip, true)
                            .field("Port", server.server_port.to_string(), true)
                            .footer(serenity::CreateEmbedFooter::new(format!("client.connect {}:{}", server.server_ip, server.server_port)));

                        let _ = channel.edit_message(&ctx.http, message_id, serenity::EditMessage::new().embed(embed)).await;
                    }
                }
            }
        }
        
        Ok(())
    } else {
        Err(anyhow::anyhow!("Server not connected."))
    }
}
