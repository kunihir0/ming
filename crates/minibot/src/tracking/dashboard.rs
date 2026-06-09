use anyhow::Result;
use db::DbPool;
use diesel::prelude::*;
use poise::serenity_prelude as serenity;

pub async fn refresh_dashboard(http: &serenity::Http, db_pool: &DbPool, server_id_filter: i32) -> Result<()> {
    let mut conn = db_pool.get()?;
    
    use db::schema::track_notifications_config::dsl as config_dsl;
    use db::schema::track_groups::dsl as groups_dsl;
    use db::schema::tracked_players::dsl as players_dsl;
    use db::schema::player_name_history::dsl as hist_dsl;
    
    let config = match config_dsl::track_notifications_config
        .filter(config_dsl::server_id.eq(server_id_filter))
        .first::<db::models::TrackNotificationsConfig>(&mut conn)
        .optional()? {
        Some(c) => c,
        None => return Ok(()),
    };
    
    let channel_id_str = match config.discord_channel_id {
        Some(c) => c,
        None => return Ok(()), // No channel configured
    };
    
    let message_id_str = match config.dashboard_message_id {
        Some(m) => m,
        None => return Ok(()), // No message configured
    };
    
    let channel_id = serenity::ChannelId::new(channel_id_str.parse()?);
    let message_id = serenity::MessageId::new(message_id_str.parse()?);
    
    let groups = groups_dsl::track_groups
        .filter(groups_dsl::server_id.eq(server_id_filter))
        .load::<db::models::TrackGroup>(&mut conn)?;
        
    let players = players_dsl::tracked_players
        .filter(players_dsl::server_id.eq(server_id_filter))
        .load::<db::models::TrackedPlayer>(&mut conn)?;
        
    let mut embed = serenity::CreateEmbed::new()
        .title("Player Tracking Dashboard")
        .color(0xCE422B)
        .timestamp(chrono::Utc::now());
        
    let mut desc = String::new();
    
    // Unassigned players
    let mut unassigned: Vec<&db::models::TrackedPlayer> = players.iter().filter(|p| p.group_id.is_none()).collect();
    unassigned.sort_by_key(|p| (p.is_online == 0, p.last_known_name.clone()));
    
    if !unassigned.is_empty() {
        desc.push_str("**Unassigned**\n```diff\n");
        for p in unassigned {
            let sign = if p.is_online == 1 { "+" } else { "-" };
            let name = p.last_known_name.as_deref().unwrap_or("Unknown");
            let server_info = if p.is_online == 1 {
                format!("{}", p.last_known_server_id.as_deref().unwrap_or("Unknown Server"))
            } else {
                "Offline".to_string()
            };
            
            // Check for aliases (last 3)
            let aliases = hist_dsl::player_name_history
                .filter(hist_dsl::tracked_player_id.eq(p.id))
                .order(hist_dsl::seen_at.desc())
                .limit(3)
                .load::<db::models::PlayerNameHistory>(&mut conn)?;
                
            let mut unique_aliases: Vec<String> = Vec::new();
            for a in aliases {
                if a.name != name && !unique_aliases.contains(&a.name) {
                    unique_aliases.push(a.name);
                }
            }
                
            desc.push_str(&format!("{} {} ({}) | {}\n", sign, name, p.steam_id, server_info));
            if !unique_aliases.is_empty() {
                desc.push_str(&format!("  Aliases: {}\n", unique_aliases.join(", ")));
            }
        }
        desc.push_str("```\n");
    }
    
    for group in groups {
        let mut group_players: Vec<&db::models::TrackedPlayer> = players.iter().filter(|p| p.group_id == Some(group.id)).collect();
        group_players.sort_by_key(|p| (p.is_online == 0, p.last_known_name.clone()));
        
        desc.push_str(&format!("**{}**\n", group.name));
        
        if group_players.is_empty() {
            desc.push_str("*No players in this group*\n\n");
            continue;
        }
        
        desc.push_str("```diff\n");
        for p in group_players {
            let sign = if p.is_online == 1 { "+" } else { "-" };
            let name = p.last_known_name.as_deref().unwrap_or("Unknown");
            let server_info = if p.is_online == 1 {
                format!("{}", p.last_known_server_id.as_deref().unwrap_or("Unknown Server"))
            } else {
                "Offline".to_string()
            };
            
            let aliases = hist_dsl::player_name_history
                .filter(hist_dsl::tracked_player_id.eq(p.id))
                .order(hist_dsl::seen_at.desc())
                .limit(3)
                .load::<db::models::PlayerNameHistory>(&mut conn)?;
                
            let mut unique_aliases: Vec<String> = Vec::new();
            for a in aliases {
                if a.name != name && !unique_aliases.contains(&a.name) {
                    unique_aliases.push(a.name);
                }
            }
                
            desc.push_str(&format!("{} {} ({}) | {}\n", sign, name, p.steam_id, server_info));
            if !unique_aliases.is_empty() {
                desc.push_str(&format!("  Aliases: {}\n", unique_aliases.join(", ")));
            }
        }
        desc.push_str("```\n");
    }
    
    if desc.is_empty() {
        desc.push_str("No players are currently being tracked. Use `/track add` to add some!\n\n");
    } else {
        desc.push_str("\n");
    }
    
    desc.push_str("```yaml\nHelp Menu:\nAdd Person: Track a new player by Steam ID\nRemove Person: Stop tracking a player\nAssign to Group: Move a player into a group\nCreate Group: Make a new group folder\nDelete Group: Remove a group and unassign its members\nClear Aliases: Reset a player's name history\n```");
    
    embed = embed.description(desc);
    
    let row1 = serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(format!("track_addperson_{}", server_id_filter))
            .label("Add Person")
            .style(serenity::ButtonStyle::Primary),
        serenity::CreateButton::new(format!("track_removeperson_{}", server_id_filter))
            .label("Remove Person")
            .style(serenity::ButtonStyle::Danger),
        serenity::CreateButton::new(format!("track_assign_{}", server_id_filter))
            .label("Assign to Group")
            .style(serenity::ButtonStyle::Secondary),
    ]);
    
    let row2 = serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(format!("track_creategroup_{}", server_id_filter))
            .label("Create Group")
            .style(serenity::ButtonStyle::Success),
        serenity::CreateButton::new(format!("track_deletegroup_{}", server_id_filter))
            .label("Delete Group")
            .style(serenity::ButtonStyle::Danger),
        serenity::CreateButton::new(format!("track_clearaliases_{}", server_id_filter))
            .label("Clear Aliases")
            .style(serenity::ButtonStyle::Secondary),
    ]);
    
    let builder = serenity::EditMessage::new().embed(embed).components(vec![row1, row2]);
    channel_id.edit_message(http, message_id, builder).await?;
    
    Ok(())
}

use serenity::builder::{CreateModal, CreateActionRow, CreateInputText, CreateInteractionResponse, CreateInteractionResponseMessage};

pub async fn handle_component(ctx: &serenity::Context, component: &serenity::ComponentInteraction, _db_pool: &DbPool) -> Result<()> {
    let custom_id = &component.data.custom_id;
    let parts: Vec<&str> = custom_id.split('_').collect();
    if parts.len() < 3 {
        return Ok(());
    }
    let action = parts[1];
    let server_id = parts[2];
    
    match action {
        "addperson" => {
            let modal = CreateModal::new(format!("track_addperson_modal_{}", server_id), "Add Person to Track")
                .components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(serenity::InputTextStyle::Short, "Steam ID 64", "steam_id")
                            .placeholder("7656119...")
                            .required(true)
                    )
                ]);
            component.create_response(&ctx.http, CreateInteractionResponse::Modal(modal)).await?;
        },
        "removeperson" => {
            let modal = CreateModal::new(format!("track_removeperson_modal_{}", server_id), "Remove Person")
                .components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(serenity::InputTextStyle::Short, "Steam ID 64", "steam_id")
                            .placeholder("7656119...")
                            .required(true)
                    )
                ]);
            component.create_response(&ctx.http, CreateInteractionResponse::Modal(modal)).await?;
        },
        "assign" => {
            let modal = CreateModal::new(format!("track_assign_modal_{}", server_id), "Assign to Group")
                .components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(serenity::InputTextStyle::Short, "Steam ID 64", "steam_id")
                            .placeholder("7656119...")
                            .required(true)
                    ),
                    CreateActionRow::InputText(
                        CreateInputText::new(serenity::InputTextStyle::Short, "Group Name", "group_name")
                            .placeholder("Enemies")
                            .required(true)
                    )
                ]);
            component.create_response(&ctx.http, CreateInteractionResponse::Modal(modal)).await?;
        },
        "creategroup" => {
            let modal = CreateModal::new(format!("track_creategroup_modal_{}", server_id), "Create Group")
                .components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(serenity::InputTextStyle::Short, "Group Name", "group_name")
                            .placeholder("Enemies")
                            .required(true)
                    )
                ]);
            component.create_response(&ctx.http, CreateInteractionResponse::Modal(modal)).await?;
        },
        "deletegroup" => {
            let modal = CreateModal::new(format!("track_deletegroup_modal_{}", server_id), "Delete Group")
                .components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(serenity::InputTextStyle::Short, "Group Name", "group_name")
                            .placeholder("Enemies")
                            .required(true)
                    )
                ]);
            component.create_response(&ctx.http, CreateInteractionResponse::Modal(modal)).await?;
        },
        "clearaliases" => {
            let modal = CreateModal::new(format!("track_clearaliases_modal_{}", server_id), "Clear Aliases")
                .components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(serenity::InputTextStyle::Short, "Steam ID 64", "steam_id")
                            .placeholder("7656119...")
                            .required(true)
                    )
                ]);
            component.create_response(&ctx.http, CreateInteractionResponse::Modal(modal)).await?;
        },
        _ => {}
    }
    
    Ok(())
}

pub async fn handle_modal(ctx: &serenity::Context, modal: &serenity::ModalInteraction, db_pool: &DbPool) -> Result<()> {
    let custom_id = &modal.data.custom_id;
    let parts: Vec<&str> = custom_id.split('_').collect();
    if parts.len() < 4 {
        return Ok(());
    }
    let action = parts[1];
    let server_id: i32 = parts[3].parse().unwrap_or(0);
    if server_id == 0 {
        return Ok(());
    }
    
    modal.create_response(&ctx.http, CreateInteractionResponse::Defer(
        CreateInteractionResponseMessage::new().ephemeral(true)
    )).await?;
    
    let get_input = |id: &str| -> Option<String> {
        for row in &modal.data.components {
            for comp in &row.components {
                if let serenity::model::application::ActionRowComponent::InputText(input) = comp {
                    if input.custom_id == id {
                        return input.value.clone();
                    }
                }
            }
        }
        None
    };

    let mut conn = match db_pool.get() {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    
    use db::schema::*;

    let mut success_msg = String::from("✅ Action completed successfully!");

    match action {
        "addperson" => {
            if let Some(steam_id) = get_input("steam_id") {
                let existing: i64 = tracked_players::dsl::tracked_players
                    .filter(tracked_players::dsl::server_id.eq(server_id))
                    .filter(tracked_players::dsl::steam_id.eq(&steam_id))
                    .count()
                    .get_result(&mut conn)?;
                if existing == 0 {
                    diesel::insert_into(tracked_players::dsl::tracked_players)
                        .values(db::models::NewTrackedPlayer {
                            group_id: None,
                            server_id,
                            steam_id: steam_id.clone(),
                            bm_player_id: None,
                            last_known_name: None,
                            last_known_server_id: None,
                            is_online: 0,
                        })
                        .execute(&mut conn)?;
                } else {
                    success_msg = format!("Player {} is already tracked.", steam_id);
                }
            }
        },
        "removeperson" => {
            if let Some(steam_id) = get_input("steam_id") {
                diesel::delete(
                    tracked_players::dsl::tracked_players
                        .filter(tracked_players::dsl::server_id.eq(server_id))
                        .filter(tracked_players::dsl::steam_id.eq(&steam_id))
                ).execute(&mut conn)?;
            }
        },
        "assign" => {
            if let (Some(steam_id), Some(group_name)) = (get_input("steam_id"), get_input("group_name")) {
                if let Ok(group) = track_groups::dsl::track_groups
                    .filter(track_groups::dsl::server_id.eq(server_id))
                    .filter(track_groups::dsl::name.eq(&group_name))
                    .first::<db::models::TrackGroup>(&mut conn)
                {
                    diesel::update(tracked_players::dsl::tracked_players
                        .filter(tracked_players::dsl::server_id.eq(server_id))
                        .filter(tracked_players::dsl::steam_id.eq(&steam_id)))
                        .set(tracked_players::dsl::group_id.eq(Some(group.id)))
                        .execute(&mut conn)?;
                } else {
                    success_msg = format!("Group '{}' not found.", group_name);
                }
            }
        },
        "creategroup" => {
            if let Some(group_name) = get_input("group_name") {
                let existing: i64 = track_groups::dsl::track_groups
                    .filter(track_groups::dsl::server_id.eq(server_id))
                    .filter(track_groups::dsl::name.eq(&group_name))
                    .count()
                    .get_result(&mut conn)?;
                if existing == 0 {
                    diesel::insert_into(track_groups::dsl::track_groups)
                        .values(db::models::NewTrackGroup {
                            server_id,
                            name: group_name,
                            color: None,
                        })
                        .execute(&mut conn)?;
                } else {
                    success_msg = format!("Group '{}' already exists.", group_name);
                }
            }
        },
        "deletegroup" => {
            if let Some(group_name) = get_input("group_name") {
                if let Ok(group) = track_groups::dsl::track_groups
                    .filter(track_groups::dsl::server_id.eq(server_id))
                    .filter(track_groups::dsl::name.eq(&group_name))
                    .first::<db::models::TrackGroup>(&mut conn)
                {
                    diesel::update(tracked_players::dsl::tracked_players.filter(tracked_players::dsl::group_id.eq(group.id)))
                        .set(tracked_players::dsl::group_id.eq(None::<i32>))
                        .execute(&mut conn)?;
                    diesel::delete(track_groups::dsl::track_groups.filter(track_groups::dsl::id.eq(group.id)))
                        .execute(&mut conn)?;
                } else {
                    success_msg = format!("Group '{}' not found.", group_name);
                }
            }
        },
        "clearaliases" => {
            if let Some(steam_id) = get_input("steam_id") {
                if let Ok(player) = tracked_players::dsl::tracked_players
                    .filter(tracked_players::dsl::server_id.eq(server_id))
                    .filter(tracked_players::dsl::steam_id.eq(&steam_id))
                    .first::<db::models::TrackedPlayer>(&mut conn)
                {
                    diesel::delete(player_name_history::dsl::player_name_history.filter(player_name_history::dsl::tracked_player_id.eq(player.id)))
                        .execute(&mut conn)?;
                } else {
                    success_msg = format!("Player {} not found.", steam_id);
                }
            }
        },
        _ => {}
    }
    
    let _ = modal.create_followup(&ctx.http, serenity::builder::CreateInteractionResponseFollowup::new().content(success_msg).ephemeral(true)).await;
    
    // Refresh the dashboard
    let _ = crate::tracking::dashboard::refresh_dashboard(&ctx.http, db_pool, server_id).await;
    
    Ok(())
}
