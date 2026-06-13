use crate::framework::{CommandResponse, UnifiedCommand, UnifiedContext};
use crate::{Error, PoiseContext};
use anyhow::Result;
use db::models::{NewTrackGroup, NewTrackedPlayer};
use db::schema::track_groups::dsl as groups_dsl;
use db::schema::tracked_players::dsl as players_dsl;
use diesel::prelude::*;
use std::future::Future;
use std::pin::Pin;

pub struct TrackCommand;

impl UnifiedCommand for TrackCommand {
    fn name(&self) -> &'static str {
        "track"
    }

    fn description(&self) -> &'static str {
        "Track a player by Steam ID"
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a UnifiedContext<'a>,
        args: &'a [&'a str],
    ) -> Pin<Box<dyn Future<Output = Result<CommandResponse>> + Send + 'a>> {
        Box::pin(async move {
            if args.is_empty() {
                return Ok(CommandResponse::text(vec![
                    "Usage: @track <steamid>".to_string(),
                ]));
            }

            let steam_id = args[0].to_string();
            
            // Just insert them into the DB; the watchdog will resolve their BM ID and Name.
            let mut conn = ctx.data.db_pool.get()?;
            
            // Check if already tracked on this server
            let existing: i64 = players_dsl::tracked_players
                .filter(players_dsl::server_id.eq(ctx.server_id))
                .filter(players_dsl::steam_id.eq(&steam_id))
                .count()
                .get_result(&mut conn)?;
                
            if existing > 0 {
                return Ok(CommandResponse::text(vec![
                    format!("Player {} is already being tracked.", steam_id),
                ]));
            }
            
            diesel::insert_into(players_dsl::tracked_players)
                .values(NewTrackedPlayer {
                    group_id: None,
                    server_id: ctx.server_id,
                    steam_id: steam_id.clone(),
                    bm_player_id: None,
                    last_known_name: None,
                    last_known_server_id: None,
                    is_online: 0,
                })
                .execute(&mut conn)?;

            if let Err(e) = crate::tracking::dashboard::refresh_dashboard(&ctx.data.discord_http, &ctx.data.db_pool, ctx.server_id).await {
                tracing::error!("Failed to refresh dashboard: {}", e);
            }

            Ok(CommandResponse::text(vec![
                format!("Now tracking Steam ID {}. The watchdog will sync their info shortly.", steam_id),
            ]))
        })
    }
}

// ---------------------------------------------------------------------------
// Discord Slash Commands
// ---------------------------------------------------------------------------

#[poise::command(slash_command, subcommands("setup_dashboard", "setup_tts", "tts_toggle", "add", "remove", "group"), subcommand_required, category = "Player Tracking")]
pub async fn track(_ctx: PoiseContext<'_>) -> Result<(), Error> {
    Ok(())
}

/// Setup the TTS voice channel for tracking notifications
#[poise::command(slash_command)]
async fn setup_tts(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
    #[description = "Voice Channel"]
    #[channel_types("Voice")]
    channel: serenity::model::channel::GuildChannel,
) -> Result<(), Error> {
    ctx.defer().await?;
    
    let mut conn = ctx.data().db_pool.get()?;
    use db::schema::track_notifications_config::dsl as config_dsl;
    use db::models::NewTrackNotificationsConfig;
    
    let existing: i64 = config_dsl::track_notifications_config
        .filter(config_dsl::server_id.eq(server_id))
        .count()
        .get_result(&mut conn)?;
        
    if existing > 0 {
        diesel::update(config_dsl::track_notifications_config.filter(config_dsl::server_id.eq(server_id)))
            .set(config_dsl::tts_voice_channel_id.eq(Some(channel.id.to_string())))
            .execute(&mut conn)?;
    } else {
        diesel::insert_into(config_dsl::track_notifications_config)
            .values(NewTrackNotificationsConfig {
                server_id,
                discord_channel_id: None,
                dashboard_message_id: None,
                tts_voice_channel_id: Some(channel.id.to_string()),
                in_game_alerts: 0,
                alert_on_join: 1,
                alert_on_leave: 1,
                alert_on_name_change: 1,
                tts_enabled: 1,
            })
            .execute(&mut conn)?;
    }
    
    ctx.say(format!("✅ TTS voice channel set to <#{}>", channel.id)).await?;
    Ok(())
}

/// Toggle TTS notifications on or off
#[poise::command(slash_command)]
async fn tts_toggle(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
) -> Result<(), Error> {
    ctx.defer().await?;
    
    let mut conn = ctx.data().db_pool.get()?;
    use db::schema::track_notifications_config::dsl as config_dsl;
    
    let config_opt = config_dsl::track_notifications_config
        .filter(config_dsl::server_id.eq(server_id))
        .first::<db::models::TrackNotificationsConfig>(&mut conn)
        .optional()?;
        
    if let Some(config) = config_opt {
        let new_state = if config.tts_enabled == 1 { 0 } else { 1 };
        diesel::update(config_dsl::track_notifications_config.filter(config_dsl::server_id.eq(server_id)))
            .set(config_dsl::tts_enabled.eq(new_state))
            .execute(&mut conn)?;
            
        let state_str = if new_state == 1 { "enabled" } else { "disabled" };
        ctx.say(format!("✅ TTS notifications are now **{}** for this server.", state_str)).await?;
    } else {
        ctx.say("❌ No tracking configuration found for this server. Please run `/track setup_tts` first.").await?;
    }
    
    Ok(())
}

/// Setup the tracking dashboard in the current channel
#[poise::command(slash_command)]
async fn setup_dashboard(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
) -> Result<(), Error> {
    ctx.defer().await?;
    
    let channel_id = ctx.channel_id();
    
    // Create an initial placeholder message
    let msg = ctx.say("Setting up Tracking Dashboard...").await?;
    let message_id = msg.message().await?.id;
    
    let mut conn = ctx.data().db_pool.get()?;
    
    use db::schema::track_notifications_config::dsl as config_dsl;
    use db::models::NewTrackNotificationsConfig;
    
    // Check if config exists
    let existing: i64 = config_dsl::track_notifications_config
        .filter(config_dsl::server_id.eq(server_id))
        .count()
        .get_result(&mut conn)?;
        
    if existing > 0 {
        diesel::update(config_dsl::track_notifications_config.filter(config_dsl::server_id.eq(server_id)))
            .set((
                config_dsl::discord_channel_id.eq(Some(channel_id.to_string())),
                config_dsl::dashboard_message_id.eq(Some(message_id.to_string())),
            ))
            .execute(&mut conn)?;
    } else {
        diesel::insert_into(config_dsl::track_notifications_config)
            .values(NewTrackNotificationsConfig {
                server_id,
                discord_channel_id: Some(channel_id.to_string()),
                dashboard_message_id: Some(message_id.to_string()),
                tts_voice_channel_id: None,
                in_game_alerts: 0,
                alert_on_join: 1,
                alert_on_leave: 1,
                alert_on_name_change: 1,
                tts_enabled: 1,
            })
            .execute(&mut conn)?;
    }
    
    // Trigger an immediate refresh
    if let Err(e) = crate::tracking::dashboard::refresh_dashboard(ctx.http(), &ctx.data().db_pool, server_id).await {
        ctx.say(format!("Dashboard initialized, but failed to render immediately: {}", e)).await?;
    }
    
    Ok(())
}

/// Add a player to the tracking list
#[poise::command(slash_command)]
async fn add(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
    #[description = "Steam ID 64"]
    steam_id: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    
    let mut conn = ctx.data().db_pool.get()?;
    
    let existing: i64 = players_dsl::tracked_players
        .filter(players_dsl::server_id.eq(server_id))
        .filter(players_dsl::steam_id.eq(&steam_id))
        .count()
        .get_result(&mut conn)?;
        
    if existing > 0 {
        ctx.say(format!("Player {} is already being tracked.", steam_id)).await?;
        return Ok(());
    }
    
    diesel::insert_into(players_dsl::tracked_players)
        .values(NewTrackedPlayer {
            group_id: None,
            server_id,
            steam_id: steam_id.clone(),
            bm_player_id: None,
            last_known_name: None,
            last_known_server_id: None,
            is_online: 0,
        })
        .execute(&mut conn)?;
        
    if let Err(e) = crate::tracking::dashboard::refresh_dashboard(ctx.http(), &ctx.data().db_pool, server_id).await {
        tracing::error!("Failed to refresh dashboard: {}", e);
    }
        
    ctx.say(format!("✅ Added {} to the tracking list. The watchdog will sync their profile soon.", steam_id)).await?;
    Ok(())
}

/// Remove a player from the tracking list
#[poise::command(slash_command)]
async fn remove(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
    #[description = "Steam ID 64"]
    steam_id: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let mut conn = ctx.data().db_pool.get()?;
    
    let deleted = diesel::delete(
        players_dsl::tracked_players
            .filter(players_dsl::server_id.eq(server_id))
            .filter(players_dsl::steam_id.eq(&steam_id))
    ).execute(&mut conn)?;
    
    if deleted > 0 {
        if let Err(e) = crate::tracking::dashboard::refresh_dashboard(ctx.http(), &ctx.data().db_pool, server_id).await {
            tracing::error!("Failed to refresh dashboard: {}", e);
        }
        ctx.say(format!("✅ Removed {} from tracking.", steam_id)).await?;
    } else {
        ctx.say("Player not found in tracking list.").await?;
    }
    
    Ok(())
}

#[poise::command(slash_command, subcommands("group_create", "group_assign"))]
pub async fn group(_ctx: PoiseContext<'_>) -> Result<(), Error> {
    Ok(())
}

/// Create a new tracking group (Clan)
#[poise::command(slash_command, rename = "create")]
async fn group_create(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
    #[description = "Group Name"]
    name: String,
) -> Result<(), Error> {
    let mut conn = ctx.data().db_pool.get()?;
    diesel::insert_into(groups_dsl::track_groups)
        .values(NewTrackGroup {
            server_id,
            name: name.clone(),
            color: None,
        })
        .execute(&mut conn)?;
        
    ctx.say(format!("✅ Created tracking group: {}", name)).await?;
    Ok(())
}

/// Assign a player to a tracking group
#[poise::command(slash_command, rename = "assign")]
async fn group_assign(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
    #[description = "Steam ID 64"]
    steam_id: String,
    #[description = "Group ID"]
    group_id: i32,
) -> Result<(), Error> {
    let mut conn = ctx.data().db_pool.get()?;
    
    let updated = diesel::update(
        players_dsl::tracked_players
            .filter(players_dsl::server_id.eq(server_id))
            .filter(players_dsl::steam_id.eq(&steam_id))
    )
    .set(players_dsl::group_id.eq(group_id))
    .execute(&mut conn)?;
    
    if updated > 0 {
        if let Err(e) = crate::tracking::dashboard::refresh_dashboard(ctx.http(), &ctx.data().db_pool, server_id).await {
            tracing::error!("Failed to refresh dashboard: {}", e);
        }
        ctx.say("✅ Player assigned to group.").await?;
    } else {
        ctx.say("Player not found on this server.").await?;
    }
    
    Ok(())
}

/// Find a player's current server using BattleMetrics
#[poise::command(slash_command, category = "Player Tracking")]
pub async fn find(
    ctx: PoiseContext<'_>,
    #[description = "Steam ID 64"] steam_id: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    
    let mut conn = ctx.data().db_pool.get()?;
    use db::schema::tracked_players::dsl as players_dsl;
    
    // Check if we have their BM ID
    let bm_id_opt: Option<String> = players_dsl::tracked_players
        .filter(players_dsl::steam_id.eq(&steam_id))
        .filter(players_dsl::bm_player_id.is_not_null())
        .select(players_dsl::bm_player_id)
        .first::<Option<String>>(&mut conn)
        .optional()?
        .flatten();
        
    let bm_id = match bm_id_opt {
        Some(id) => id,
        None => {
            // Try to resolve using Atlas
            match crate::tracking::atlas::client::AtlasClient::new() {
                Ok(client) => {
                    match client.get_player(&steam_id).await {
                        Ok(res) => {
                            if let Some(ap) = res.player {
                                if let Some(atlas_bm_id) = ap.bm_player_id {
                                    let atlas_bm_str = atlas_bm_id.to_string();
                                    // Cache it
                                    let _ = db::upsert_player_link(&mut conn, &steam_id, &atlas_bm_str);
                                    atlas_bm_str
                                } else {
                                    ctx.say(format!("❌ Could not find a BattleMetrics ID for Steam ID `{}` on Atlas.", steam_id)).await?;
                                    return Ok(());
                                }
                            } else {
                                ctx.say(format!("❌ Player not found on Atlas Rust. Cannot resolve BM ID.")).await?;
                                return Ok(());
                            }
                        }
                        Err(e) => {
                            ctx.say(format!("❌ Failed to query Atlas API to resolve BM ID: {}", e)).await?;
                            return Ok(());
                        }
                    }
                }
                Err(_) => {
                    ctx.say(format!("❌ Player is not tracked and Atlas API is not configured. Cannot resolve BM ID.")).await?;
                    return Ok(());
                }
            }
        }
    };
    
    // Scrape BM profile
    let bm_client = crate::tracking::battlemetrics::client::BmScraperClient::new();
    match bm_client.scrape_player_profile(&bm_id).await {
        Ok(bm_player) => {
            if bm_player.is_online {
                if let Some(server_id) = bm_player.current_server_id {
                    let server_name = bm_client.get_server_name(&server_id).await.unwrap_or_else(|_| "Unknown Server".to_string());
                    ctx.say(format!("✅ **{}** is currently **ONLINE** on **{}** (Server ID: {})", bm_player.current_name, server_name, server_id)).await?;
                } else {
                    ctx.say(format!("✅ **{}** is currently **ONLINE**, but their current server is hidden or unavailable.", bm_player.current_name)).await?;
                }
            } else {
                ctx.say(format!("❌ **{}** is currently **OFFLINE**.", bm_player.current_name)).await?;
            }
        }
        Err(e) => {
            ctx.say(format!("❌ Failed to fetch BattleMetrics profile for BM ID `{}`: {}", bm_id, e)).await?;
        }
    }
    
    Ok(())
}
