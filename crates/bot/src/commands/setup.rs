use crate::db::models::GuildConfig;
use crate::db::schema::guild_configs::dsl::{guild_configs, guild_id as gc_guild_id};
use crate::{Context, Error};
use diesel::prelude::*;
use poise::serenity_prelude as serenity;

#[derive(Debug, poise::ChoiceParameter)]
pub enum SetupModeChoice {
    #[name = "Auto-Create Categories"]
    Auto,
    #[name = "Use Existing Channels"]
    Manual,
}

/// Setup the bot for this server
#[poise::command(slash_command, required_permissions = "ADMINISTRATOR")]
pub async fn setup(
    ctx: Context<'_>,
    #[description = "Setup mode"] mode: SetupModeChoice,
    #[description = "Dashboard Channel (Manual mode only)"] dashboard_channel: Option<
        serenity::Channel,
    >,
    #[description = "Chat Channel (Manual mode only)"] chat_channel: Option<serenity::Channel>,
    #[description = "Alerts Channel (Manual mode only)"] alerts_channel: Option<serenity::Channel>,
) -> Result<(), Error> {
    let guild_id_str = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();
    let management_channel_id = ctx.channel_id().to_string();

    let mut conn = ctx.data().db_pool.get()?;

    match mode {
        SetupModeChoice::Auto => {
            let config = GuildConfig {
                guild_id: guild_id_str.clone(),
                setup_mode: "Auto".to_string(),
                manual_dashboard_channel_id: None,
                manual_chat_channel_id: None,
                manual_alerts_channel_id: None,
                in_game_prefix: "!".to_string(),
                management_channel_id: Some(management_channel_id),
            };

            diesel::insert_into(guild_configs)
                .values(&config)
                .on_conflict(gc_guild_id)
                .do_update()
                .set(&config)
                .execute(&mut conn)?;

            ctx.say("Setup complete! Mode set to Auto-Create Categories. This channel will now receive pairing requests.")
                .await?;
        }
        SetupModeChoice::Manual => {
            let dashboard =
                dashboard_channel.ok_or("Dashboard channel is required for manual mode")?;
            let chat = chat_channel.ok_or("Chat channel is required for manual mode")?;
            let alerts = alerts_channel.ok_or("Alerts channel is required for manual mode")?;

            let config = GuildConfig {
                guild_id: guild_id_str.clone(),
                setup_mode: "Manual".to_string(),
                manual_dashboard_channel_id: Some(dashboard.id().to_string()),
                manual_chat_channel_id: Some(chat.id().to_string()),
                manual_alerts_channel_id: Some(alerts.id().to_string()),
                in_game_prefix: "!".to_string(),
                management_channel_id: Some(management_channel_id),
            };

            diesel::insert_into(guild_configs)
                .values(&config)
                .on_conflict(gc_guild_id)
                .do_update()
                .set(&config)
                .execute(&mut conn)?;

            ctx.say("Setup complete! Mode set to Manual with the provided channels. This channel will now receive pairing requests.")
                .await?;
        }
    }

    Ok(())
}
