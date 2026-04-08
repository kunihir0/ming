use crate::Data;
use crate::db::DbPool;
use crate::db::models::{PairedServer, ServerChannel, ServerSettings};
use crate::db::schema::paired_servers::dsl as ps_dsl;
use crate::db::schema::server_channels::dsl as sc_dsl;
use crate::db::schema::server_settings::dsl as ss_dsl;
use diesel::prelude::*;
use poise::serenity_prelude as serenity;

/// Updates the configuration dashboard message in Discord.
///
/// # Errors
/// Returns an error if the database query fails or the Discord API call fails.
pub async fn update_config_dashboard(
    http: impl serenity::CacheHttp,
    db_pool: &DbPool,
    server_id: i32,
) -> anyhow::Result<()> {
    let mut conn = db_pool.get()?;
    let server: PairedServer = ps_dsl::paired_servers.find(server_id).first(&mut conn)?;
    let server_channel: ServerChannel = sc_dsl::server_channels.find(server_id).first(&mut conn)?;
    let settings: ServerSettings = ss_dsl::server_settings.find(server_id).first(&mut conn)?;

    let (Some(channel_id_str), Some(msg_id_str)) = (
        server_channel.config_channel_id,
        server_channel.config_message_id,
    ) else {
        return Ok(());
    };

    let channel_id = serenity::ChannelId::new(channel_id_str.parse::<u64>()?);
    let message_id = serenity::MessageId::new(msg_id_str.parse::<u64>()?);

    let embed = serenity::CreateEmbed::new()
        .title(format!("Bot Settings - {}", server.name))
        .color(0x00FF_A500) // Orange
        .description("Configure how the bot behaves for this server.")
        .field("In-Game Prefix", &settings.in_game_prefix, true)
        .field(
            "Rust -> Discord Bridge",
            if settings.bridge_rust_to_discord == 1 {
                "✅ Enabled"
            } else {
                "❌ Disabled"
            },
            true,
        )
        .field(
            "Discord -> Rust Bridge",
            if settings.bridge_discord_to_rust == 1 {
                "✅ Enabled"
            } else {
                "❌ Disabled"
            },
            true,
        )
        .field(
            "Command Cooldown",
            format!("{}s", settings.command_cooldown),
            true,
        )
        .field(
            "Chat Cooldown",
            format!("{}s", settings.chat_cooldown),
            true,
        );

    let row1 = serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(format!("config_toggle_r2d_{server_id}"))
            .label("Toggle R -> D")
            .style(if settings.bridge_rust_to_discord == 1 {
                serenity::ButtonStyle::Success
            } else {
                serenity::ButtonStyle::Danger
            }),
        serenity::CreateButton::new(format!("config_toggle_d2r_{server_id}"))
            .label("Toggle D -> R")
            .style(if settings.bridge_discord_to_rust == 1 {
                serenity::ButtonStyle::Success
            } else {
                serenity::ButtonStyle::Danger
            }),
    ]);

    let row2 = serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(format!("config_edit_{server_id}"))
            .label("Edit Values (Prefix/Cooldowns)")
            .style(serenity::ButtonStyle::Primary),
    ]);

    channel_id
        .edit_message(
            &http,
            message_id,
            serenity::EditMessage::new()
                .embed(embed)
                .components(vec![row1, row2]),
        )
        .await?;

    Ok(())
}

/// Handles setting toggles and opening the edit modal.
///
/// # Errors
/// Returns an error if the database update fails or the Discord API call fails.
pub async fn handle_config_interaction(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
    data: &Data,
) -> anyhow::Result<()> {
    let custom_id = &interaction.data.custom_id;

    if custom_id.starts_with("config_toggle_") {
        let parts: Vec<&str> = custom_id.split('_').collect();
        let server_id: i32 = parts[3].parse()?;
        let field = parts[2];

        let mut conn = data.db_pool.get()?;

        if field == "r2d" {
            let current: i32 = ss_dsl::server_settings
                .find(server_id)
                .select(ss_dsl::bridge_rust_to_discord)
                .first(&mut conn)?;
            diesel::update(ss_dsl::server_settings.find(server_id))
                .set(ss_dsl::bridge_rust_to_discord.eq(i32::from(current != 1)))
                .execute(&mut conn)?;
        } else if field == "d2r" {
            let current: i32 = ss_dsl::server_settings
                .find(server_id)
                .select(ss_dsl::bridge_discord_to_rust)
                .first(&mut conn)?;
            diesel::update(ss_dsl::server_settings.find(server_id))
                .set(ss_dsl::bridge_discord_to_rust.eq(i32::from(current != 1)))
                .execute(&mut conn)?;
        }

        interaction.defer(&ctx.http).await?;
        update_config_dashboard(&ctx.http, &data.db_pool, server_id).await?;
    } else if custom_id.starts_with("config_edit_") {
        let server_id: i32 = match custom_id.split('_').next_back() {
            Some(id) => id.parse()?,
            None => "0".parse()?,
        };

        let mut conn = data.db_pool.get()?;
        let settings: ServerSettings = ss_dsl::server_settings.find(server_id).first(&mut conn)?;

        let modal =
            serenity::CreateModal::new(format!("config_modal_{server_id}"), "Edit Bot Settings")
                .components(vec![
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(
                            serenity::InputTextStyle::Short,
                            "In-Game Prefix",
                            "prefix",
                        )
                        .value(settings.in_game_prefix)
                        .max_length(5),
                    ),
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(
                            serenity::InputTextStyle::Short,
                            "Command Cooldown (seconds)",
                            "cmd_cd",
                        )
                        .value(settings.command_cooldown.to_string()),
                    ),
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(
                            serenity::InputTextStyle::Short,
                            "Chat Cooldown (seconds)",
                            "chat_cd",
                        )
                        .value(settings.chat_cooldown.to_string()),
                    ),
                ]);

        interaction
            .create_response(&ctx.http, serenity::CreateInteractionResponse::Modal(modal))
            .await?;
    }

    Ok(())
}

/// Handles the modal submission for bot settings.
///
/// # Errors
/// Returns an error if parsing fails, database update fails, or Discord API call fails.
pub async fn handle_modal_submit(
    ctx: &serenity::Context,
    modal: &serenity::ModalInteraction,
    data: &Data,
) -> anyhow::Result<()> {
    let custom_id = &modal.data.custom_id;

    if custom_id.starts_with("config_modal_") {
        let server_id: i32 = match custom_id.split('_').next_back() {
            Some(id) => id.parse()?,
            None => "0".parse()?,
        };

        let mut prefix = "!".to_string();
        let mut cmd_cd = 0;
        let mut chat_cd = 0;

        for row in &modal.data.components {
            for component in &row.components {
                if let serenity::ActionRowComponent::InputText(it) = component {
                    match it.custom_id.as_str() {
                        "prefix" => {
                            prefix = match it.value.clone() {
                                Some(p) => p,
                                None => "!".to_string(),
                            };
                        }
                        "cmd_cd" => {
                            cmd_cd = match it.value.as_ref().and_then(|v| v.parse().ok()) {
                                Some(c) => c,
                                None => 0,
                            };
                        }
                        "chat_cd" => {
                            chat_cd = match it.value.as_ref().and_then(|v| v.parse().ok()) {
                                Some(c) => c,
                                None => 0,
                            };
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut conn = data.db_pool.get()?;

        diesel::update(ss_dsl::server_settings.find(server_id))
            .set((
                ss_dsl::in_game_prefix.eq(prefix),
                ss_dsl::command_cooldown.eq(cmd_cd),
                ss_dsl::chat_cooldown.eq(chat_cd),
            ))
            .execute(&mut conn)?;

        modal.defer(&ctx.http).await?;
        update_config_dashboard(&ctx.http, &data.db_pool, server_id).await?;
    }

    Ok(())
}
