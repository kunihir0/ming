#![allow(clippy::pedantic)]
#![allow(clippy::items_after_statements)]

use crate::db::DbPool;
use crate::db::models::ServerChannel as DbServerChannel;
use crate::db::models::{GuildConfig, PairedServer};
use crate::db::schema::server_channels::dsl::server_channels;
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use tracing::{error, info};

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

    let (category_id, dashboard_id, chat_id, alerts_id) = if config.setup_mode == "Auto" {
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

        (
            Some(category.id.get().to_string()),
            Some(dashboard_channel.id.get().to_string()),
            Some(chat_channel.id.get().to_string()),
            Some(alerts_channel.id.get().to_string()),
        )
    } else {
        info!("Using manual channels for server: {}", server.name);
        (
            None,
            config.manual_dashboard_channel_id.clone(),
            config.manual_chat_channel_id.clone(),
            config.manual_alerts_channel_id.clone(),
        )
    };

    let Some(dash_id_str) = &dashboard_id else {
        error!(
            "No dashboard channel ID resolved for server {}",
            server.name
        );
        return Err(anyhow::anyhow!("Missing dashboard channel"));
    };

    let dash_channel_id = serenity::ChannelId::new(dash_id_str.parse::<u64>()?);

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
    };

    diesel::insert_into(server_channels)
        .values(&new_channels)
        .execute(&mut conn)?;

    info!("Dashboard setup complete for server {}", server.name);

    Ok(())
}
