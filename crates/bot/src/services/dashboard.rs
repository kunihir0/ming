#![allow(clippy::pedantic)]
#![allow(clippy::items_after_statements)]

use crate::db::DbPool;
use crate::db::models::ServerChannel as DbServerChannel;
use crate::db::models::{GuildConfig, PairedServer, ServerChannel};
use crate::db::schema::paired_servers::dsl as ps_dsl;
use crate::db::schema::server_channels::dsl as sc_dsl;
use crate::db::schema::server_channels::dsl::server_channels;
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use rustplus::proto::{AppInfo, AppTeamInfo};
use tracing::{error, info, warn};

/// Handles the setup of a newly paired server, creating channels or using existing ones
///
/// # Errors
/// Returns an error if the database query fails, channel creation fails, or message sending fails.
pub async fn handle_new_paired_server(
    db_pool: &DbPool,
    ctx: &serenity::Context,
    guild_id_str: &str,
    server: &PairedServer,
) -> anyhow::Result<()> {
    use crate::db::schema::guild_configs::dsl::guild_configs;

    let mut conn = db_pool.get()?;

    let config: GuildConfig = if let Ok(c) = guild_configs.find(guild_id_str).first(&mut conn) {
        c
    } else {
        error!("Guild config not found for guild_id: {}", guild_id_str);
        return Err(anyhow::anyhow!("Guild config not found"));
    };

    let guild_id = guild_id_str.parse::<u64>()?;
    let guild_id = serenity::GuildId::new(guild_id);

    let (category_id, dashboard_id, chat_id, alerts_id, config_id) = if config.setup_mode == "Auto"
    {
        info!("Auto-creating channels for server: {}", server.name);

        let category_name = format!("Rust - {}", server.name);

        let category = guild_id
            .create_channel(
                &ctx.http,
                serenity::CreateChannel::new(category_name).kind(serenity::ChannelType::Category),
            )
            .await?;

        let dashboard_channel = guild_id
            .create_channel(
                &ctx.http,
                serenity::CreateChannel::new("dashboard")
                    .kind(serenity::ChannelType::Text)
                    .category(category.id),
            )
            .await?;

        let chat_channel = guild_id
            .create_channel(
                &ctx.http,
                serenity::CreateChannel::new("team-chat")
                    .kind(serenity::ChannelType::Text)
                    .category(category.id),
            )
            .await?;

        let alerts_channel = guild_id
            .create_channel(
                &ctx.http,
                serenity::CreateChannel::new("alerts")
                    .kind(serenity::ChannelType::Text)
                    .category(category.id),
            )
            .await?;

        let config_channel = guild_id
            .create_channel(
                &ctx.http,
                serenity::CreateChannel::new("config")
                    .kind(serenity::ChannelType::Text)
                    .category(category.id),
            )
            .await?;

        (
            Some(category.id.get().to_string()),
            Some(dashboard_channel.id.get().to_string()),
            Some(chat_channel.id.get().to_string()),
            Some(alerts_channel.id.get().to_string()),
            Some(config_channel.id.get().to_string()),
        )
    } else {
        info!("Using manual channels for server: {}", server.name);
        (
            None,
            config.manual_dashboard_channel_id.clone(),
            config.manual_chat_channel_id.clone(),
            config.manual_alerts_channel_id.clone(),
            None,
        )
    };

    let Some(dash_id_str) = &dashboard_id else {
        error!(
            "No dashboard channel ID resolved for server {}",
            server.name
        );
        return Err(anyhow::anyhow!("Missing dashboard channel"));
    };

    let dash_channel_id = serenity::ChannelId::new(
        dash_id_str
            .parse::<u64>()
            .map_err(|e| anyhow::anyhow!("Invalid dashboard ID: {e}"))?,
    );

    // Initialize Default Settings
    use crate::db::models::NewServerSettings;
    use crate::db::schema::server_settings::dsl::server_settings;
    let default_settings = NewServerSettings {
        server_id: server.id,
        in_game_prefix: "!".to_string(),
        bridge_rust_to_discord: 1,
        bridge_discord_to_rust: 1,
        command_cooldown: 0,
        chat_cooldown: 0,
    };
    diesel::insert_into(server_settings)
        .values(&default_settings)
        .execute(&mut conn)?;

    // Send initial config embed if channel exists
    let mut config_msg_id = None;
    if let Some(ref cfg_id_str) = config_id {
        let cfg_channel_id = serenity::ChannelId::new(
            cfg_id_str
                .parse::<u64>()
                .map_err(|e| anyhow::anyhow!("Invalid config ID: {e}"))?,
        );
        let msg = cfg_channel_id
            .send_message(
                &ctx.http,
                serenity::CreateMessage::new().content("Initializing settings..."),
            )
            .await?;
        config_msg_id = Some(msg.id.get().to_string());
    }

    // Send initial offline dashboard embed
    let embed = serenity::CreateEmbed::new()
        .title(format!("{} Dashboard", server.name))
        .color(0x00FF_0000) // Red for offline initially
        .description("Status: 🔴 **Offline**\n*Click Connect to start the Rust+ bridge.*")
        .field("Server IP", &server.server_ip, true)
        .field("Port", server.server_port.to_string(), true)
        .footer(serenity::CreateEmbedFooter::new(format!(
            "client.connect {}:{}",
            server.server_ip, server.server_port
        )));

    let connect_btn = serenity::CreateButton::new(format!("connect_{}", server.id))
        .label("Connect")
        .style(serenity::ButtonStyle::Success);

    let disconnect_btn = serenity::CreateButton::new(format!("disconnect_{}", server.id))
        .label("Disconnect")
        .style(serenity::ButtonStyle::Danger);

    let components = serenity::CreateActionRow::Buttons(vec![connect_btn, disconnect_btn]);

    let message = dash_channel_id
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .embed(embed)
                .components(vec![components]),
        )
        .await?;

    let new_channels = DbServerChannel {
        server_id: server.id,
        category_id,
        dashboard_channel_id: dashboard_id,
        chat_channel_id: chat_id,
        alerts_channel_id: alerts_id,
        dashboard_message_id: Some(message.id.get().to_string()),
        config_channel_id: config_id,
        config_message_id: config_msg_id,
    };

    diesel::insert_into(server_channels)
        .values(&new_channels)
        .execute(&mut conn)?;

    if new_channels.config_channel_id.is_some() {
        let _ = crate::services::config_dashboard::update_config_dashboard(
            &ctx.http, db_pool, server.id,
        )
        .await;
    }

    info!("Dashboard setup complete for server {}", server.name);

    Ok(())
}

pub async fn reset_all_dashboards_offline(
    http: impl serenity::CacheHttp,
    db_pool: &DbPool,
) -> anyhow::Result<()> {
    let mut conn = db_pool.get()?;
    let servers: Vec<PairedServer> = ps_dsl::paired_servers.load(&mut conn)?;

    for server in servers {
        let _ = update_dashboard_offline(&http, db_pool, server.id).await;
    }

    Ok(())
}

pub async fn update_dashboard_online(
    http: impl serenity::CacheHttp,
    db_pool: &DbPool,
    server_id: i32,
    server_info: Option<&AppInfo>,
    team_info: Option<&AppTeamInfo>,
) -> anyhow::Result<()> {
    let mut conn = db_pool.get()?;
    let server: PairedServer = ps_dsl::paired_servers.find(server_id).first(&mut conn)?;
    let server_channel: ServerChannel = sc_dsl::server_channels.find(server_id).first(&mut conn)?;

    let (Some(channel_id_str), Some(msg_id_str)) = (
        server_channel.dashboard_channel_id,
        server_channel.dashboard_message_id,
    ) else {
        return Ok(());
    };

    let channel_id = serenity::ChannelId::new(channel_id_str.parse::<u64>()?);
    let message_id = serenity::MessageId::new(msg_id_str.parse::<u64>()?);

    // Check if channel exists before editing
    if let Err(e) = channel_id.to_channel(&http).await {
        warn!("Failed to fetch dashboard channel {channel_id}: {e}. It might have been deleted manually.");
        return Ok(());
    }

    // 1. Server Info Embed
    let mut server_embed = serenity::CreateEmbed::new()
        .title(format!("{} Dashboard", server.name))
        .color(0x0000_FF00) // Green
        .field("Server IP", &server.server_ip, true)
        .field("Port", server.server_port.to_string(), true)
        .footer(serenity::CreateEmbedFooter::new(format!(
            "client.connect {}:{}",
            server.server_ip, server.server_port
        )));

    if let Some(info) = server_info {
        server_embed = server_embed
            .thumbnail(&info.header_image)
            .description("Status: 🟢 **Online**")
            .field(
                "Players",
                format!(
                    "{}/{}{}",
                    info.players,
                    info.max_players,
                    if info.queued_players > 0 {
                        format!(" ({} in queue)", info.queued_players)
                    } else {
                        String::new()
                    }
                ),
                true,
            )
            .field("Map", &info.map, true);
    } else {
        server_embed = server_embed.description("Status: 🟢 **Online** (Updating data...)");
    }

    // 2. Team UI Embed
    let mut team_content = String::new();
    if let Some(ti) = team_info {
        for member in &ti.members {
            let online_icon = if member.is_online { "🟢" } else { "🔴" };
            let life_icon = if member.is_alive {
                "Alive"
            } else {
                "💀 Dead"
            };
            let leader_icon = if member.steam_id == ti.leader_steam_id {
                "👑 "
            } else {
                ""
            };

            team_content.push_str(&format!(
                "{} {}{} - {}\n",
                online_icon, leader_icon, member.name, life_icon
            ));
        }

        if team_content.is_empty() {
            team_content = "_No team members found._".to_string();
        }
    } else {
        team_content = "_Fetching team data..._".to_string();
    }

    let team_embed = serenity::CreateEmbed::new()
        .title("Team UI")
        .color(0x0000_FFFF) // Blue
        .description(team_content);

    let connect_btn = serenity::CreateButton::new(format!("connect_{}", server.id))
        .label("Connect")
        .style(serenity::ButtonStyle::Success)
        .disabled(true);

    let disconnect_btn = serenity::CreateButton::new(format!("disconnect_{}", server.id))
        .label("Disconnect")
        .style(serenity::ButtonStyle::Danger);

    let components = serenity::CreateActionRow::Buttons(vec![connect_btn, disconnect_btn]);

    channel_id
        .edit_message(
            &http,
            message_id,
            serenity::EditMessage::new()
                .embeds(vec![server_embed, team_embed])
                .components(vec![components]),
        )
        .await?;

    Ok(())
}

pub async fn update_dashboard_offline(
    http: impl serenity::CacheHttp,
    db_pool: &DbPool,
    server_id: i32,
) -> anyhow::Result<()> {
    let mut conn = db_pool.get()?;
    let server: PairedServer = ps_dsl::paired_servers.find(server_id).first(&mut conn)?;
    let server_channel: ServerChannel = sc_dsl::server_channels.find(server_id).first(&mut conn)?;

    let (Some(channel_id_str), Some(msg_id_str)) = (
        server_channel.dashboard_channel_id,
        server_channel.dashboard_message_id,
    ) else {
        return Ok(());
    };

    let channel_id = serenity::ChannelId::new(channel_id_str.parse::<u64>()?);
    let message_id = serenity::MessageId::new(msg_id_str.parse::<u64>()?);

    let embed = serenity::CreateEmbed::new()
        .title(format!("{} Dashboard", server.name))
        .color(0x00FF_0000) // Red
        .description("Status: 🔴 **Offline**\n*Click Connect to start the Rust+ bridge.*")
        .field("Server IP", &server.server_ip, true)
        .field("Port", server.server_port.to_string(), true)
        .footer(serenity::CreateEmbedFooter::new(format!(
            "client.connect {}:{}",
            server.server_ip, server.server_port
        )));

    let connect_btn = serenity::CreateButton::new(format!("connect_{}", server.id))
        .label("Connect")
        .style(serenity::ButtonStyle::Success);

    let disconnect_btn = serenity::CreateButton::new(format!("disconnect_{}", server.id))
        .label("Disconnect")
        .style(serenity::ButtonStyle::Danger)
        .disabled(true);

    let components = serenity::CreateActionRow::Buttons(vec![connect_btn, disconnect_btn]);

    channel_id
        .edit_message(
            &http,
            message_id,
            serenity::EditMessage::new()
                .embed(embed)
                .components(vec![components]),
        )
        .await?;

    Ok(())
}
