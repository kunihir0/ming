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
        desc.push_str("No players are currently being tracked. Use `/track add` to add some!");
    }
    
    embed = embed.description(desc);
    
    let builder = serenity::EditMessage::new().embed(embed);
    channel_id.edit_message(http, message_id, builder).await?;
    
    Ok(())
}
