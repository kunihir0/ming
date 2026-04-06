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

    // Update Dashboard to Online
    if let Some(ref sc) = server_channel {
        if let (Some(channel_id_str), Some(msg_id_str)) =
            (&sc.dashboard_channel_id, &sc.dashboard_message_id)
        {
            if let (Ok(channel_id), Ok(msg_id)) =
                (channel_id_str.parse::<u64>(), msg_id_str.parse::<u64>())
            {
                let channel = serenity::ChannelId::new(channel_id);
                let message_id = serenity::MessageId::new(msg_id);

                let embed = serenity::CreateEmbed::new()
                    .title(format!("{} Dashboard", server.name))
                    .color(0x0000_FF00) // Green
                    .description("Status: 🟢 **Online**")
                    .field("Server IP", &server.server_ip, true)
                    .field("Port", server.server_port.to_string(), true)
                    .footer(serenity::CreateEmbedFooter::new(format!(
                        "client.connect {}:{}",
                        server.server_ip, server.server_port
                    )));

                let _ = channel
                    .edit_message(
                        &ctx.http,
                        message_id,
                        serenity::EditMessage::new().embed(embed),
                    )
                    .await;
            }
        }
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

    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            // Handle AppTeamMessage, AppEntityChanged, etc
            if let Some(team_msg) = msg.broadcast.as_ref().and_then(|b| b.team_message.as_ref()) {
                let message_text = team_msg.message.message.clone();
                let sender_name = team_msg.message.name.clone();

                info!(
                    "[{}] Team Chat: {}: {}",
                    server_name, sender_name, message_text
                );

                if !message_text.starts_with(&prefix) && !message_text.starts_with("[Discord]") {
                    if let Some(ref channel_id_str) = chat_channel_id {
                        if let Ok(channel_id) = channel_id_str.parse::<u64>() {
                            let channel = serenity::ChannelId::new(channel_id);
                            let discord_msg = format!("**[{}]**: {}", sender_name, message_text);
                            if let Err(e) = channel.say(&ctx_clone.http, discord_msg).await {
                                error!("Failed to send team message to Discord: {}", e);
                            }
                        }
                    }
                }
            }
        }

        warn!("Rust+ connection to {} lost.", server_name);
        clients_arc.lock().await.remove(&server_id);

        // Update Dashboard to Offline
        if let Ok(mut conn) = pool_clone.get() {
            if let Ok(server_channel) = sc_dsl::server_channels
                .find(server_id)
                .first::<ServerChannel>(&mut conn)
            {
                if let (Some(channel_id_str), Some(msg_id_str)) = (
                    server_channel.dashboard_channel_id,
                    server_channel.dashboard_message_id,
                ) {
                    if let (Ok(channel_id), Ok(msg_id)) =
                        (channel_id_str.parse::<u64>(), msg_id_str.parse::<u64>())
                    {
                        let channel = serenity::ChannelId::new(channel_id);
                        let message_id = serenity::MessageId::new(msg_id);

                        let embed = serenity::CreateEmbed::new()
                            .title(format!("{} Dashboard", server_name))
                            .color(0x00FF_0000) // Red
                            .description("Status: 🔴 **Offline**")
                            .field("Server IP", "???", true)
                            .field("Port", "???", true);

                        let _ = channel
                            .edit_message(
                                &ctx_clone.http,
                                message_id,
                                serenity::EditMessage::new().embed(embed),
                            )
                            .await;
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
        if let Ok(server) = ps_dsl::paired_servers
            .find(server_id)
            .first::<PairedServer>(&mut conn)
        {
            // Update Dashboard to Offline
            if let Ok(server_channel) = sc_dsl::server_channels
                .find(server_id)
                .first::<ServerChannel>(&mut conn)
            {
                if let (Some(channel_id_str), Some(msg_id_str)) = (
                    server_channel.dashboard_channel_id,
                    server_channel.dashboard_message_id,
                ) {
                    if let (Ok(channel_id), Ok(msg_id)) =
                        (channel_id_str.parse::<u64>(), msg_id_str.parse::<u64>())
                    {
                        let channel = serenity::ChannelId::new(channel_id);
                        let message_id = serenity::MessageId::new(msg_id);

                        let embed = serenity::CreateEmbed::new()
                            .title(format!("{} Dashboard", server.name))
                            .color(0x00FF_0000) // Red
                            .description("Status: 🔴 **Offline**\n*Click Connect to start the Rust+ bridge.*")
                            .field("Server IP", &server.server_ip, true)
                            .field("Port", server.server_port.to_string(), true)
                            .footer(serenity::CreateEmbedFooter::new(format!("client.connect {}:{}", server.server_ip, server.server_port)));

                        let _ = channel
                            .edit_message(
                                &ctx.http,
                                message_id,
                                serenity::EditMessage::new().embed(embed),
                            )
                            .await;
                    }
                }
            }
        }

        Ok(())
    } else {
        Err(anyhow::anyhow!("Server not connected."))
    }
}
