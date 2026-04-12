use crate::db::models::PairedServer;
use crate::db::schema::{fcm_credentials::dsl as fcm_dsl, paired_servers::dsl as ps_dsl};
use crate::{Context, Error};
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use std::collections::HashSet;

/// Investigate players to find probable teammates
#[allow(clippy::too_many_lines)]
#[poise::command(slash_command, guild_only)]
pub async fn team_check(
    ctx: Context<'_>,
    #[description = "Optional Battlemetrics Server ID"] bm_server_id: Option<String>,
    #[description = "First SteamID or Profile URL (Required)"] target_1: Option<String>,
    #[description = "Second SteamID or Profile URL"] target_2: Option<String>,
    #[description = "Third SteamID or Profile URL"] target_3: Option<String>,
    #[description = "Fourth SteamID or Profile URL"] target_4: Option<String>,
    #[description = "Fifth SteamID or Profile URL"] target_5: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let Some(t1) = target_1 else {
        ctx.say("❌ You must provide at least one target SteamID or Profile URL.")
            .await?;
        return Ok(());
    };

    let bm_server = if let Some(ref server_id) = bm_server_id {
        if let Some(s) = ctx.data().battlemetrics.get_server_by_id(server_id).await? {
            s
        } else {
            ctx.say(format!(
                "❌ Could not find a Battlemetrics server with ID: {server_id}"
            ))
            .await?;
            return Ok(());
        }
    } else {
        let Some(guild_id) = ctx.guild_id() else {
            ctx.say("❌ This command must be run in a server unless you provide a Battlemetrics Server ID.").await?;
            return Ok(());
        };
        let guild_id_str = guild_id.to_string();

        let mut conn = ctx.data().db_pool.get()?;
        let server: Option<PairedServer> = ps_dsl::paired_servers
            .inner_join(fcm_dsl::fcm_credentials)
            .filter(fcm_dsl::guild_id.eq(&guild_id_str))
            .select(ps_dsl::paired_servers::all_columns())
            .first(&mut conn)
            .optional()?;

        let Some(paired_server) = server else {
            ctx.say("❌ No paired Rust server found for this Discord server. Please provide a Battlemetrics Server ID instead.")
                .await?;
            return Ok(());
        };

        if let Some(s) = ctx
            .data()
            .battlemetrics
            .get_server_by_address(&paired_server.server_ip, paired_server.server_port)
            .await?
        {
            s
        } else {
            ctx.say("❌ Could not find the paired Rust server on Battlemetrics.")
                .await?;
            return Ok(());
        }
    };

    let targets = vec![Some(t1), target_2, target_3, target_4, target_5]
        .into_iter()
        .flatten()
        .collect::<Vec<String>>();

    let mut all_friends_names = HashSet::new();
    let mut profiles_info = String::new();

    for target in targets {
        let profile_res = ctx.data().steam_service.get_profile(&target).await;
        match profile_res {
            Ok(profile) => {
                let visibility_str = format!("{:?}", profile.visibility);
                let _ = std::fmt::Write::write_fmt(
                    &mut profiles_info,
                    format_args!(
                        "- [{}]({}{}) ({} | Lvl {})\n",
                        profile.persona_name,
                        "https://steamcommunity.com/profiles/",
                        profile.steam_id64,
                        visibility_str,
                        profile.level
                    ),
                );

                let friends = ctx
                    .data()
                    .steam_service
                    .get_friends(&target)
                    .await
                    .unwrap_or_default();
                for friend in friends {
                    all_friends_names.insert(friend.persona_name.to_lowercase());
                }
            }
            Err(_) => {
                let _ = std::fmt::Write::write_fmt(
                    &mut profiles_info,
                    format_args!("- ❌ Failed to fetch profile for `{target}`\n"),
                );
            }
        }
    }

    let active_players = ctx
        .data()
        .battlemetrics
        .get_active_players(&bm_server.id)
        .await
        .unwrap_or_default();

    let mut active_associates = Vec::new();
    for player in active_players {
        if all_friends_names.contains(&player.to_lowercase()) {
            active_associates.push(player);
        }
    }

    let associates_info = if active_associates.is_empty() {
        "No direct associates identified in the active server roster.".to_string()
    } else {
        let mut info = format!(
            "Total Network Size: {} unique friends.\nActive on Server: {}\n\n",
            all_friends_names.len(),
            active_associates.len()
        );
        for associate in active_associates {
            let _ = std::fmt::Write::write_fmt(&mut info, format_args!("- {associate}\n"));
        }
        info
    };

    let embed = serenity::CreateEmbed::new()
        .title("Network Analysis Report")
        .color(0x0080_8080)
        .field("Analyzed Targets", profiles_info, false)
        .field("Active Associates Identified", associates_info, false)
        .footer(serenity::CreateEmbedFooter::new(format!(
            "Data cross-referenced via Battlemetrics | Server: {}",
            bm_server.attributes.name
        )));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;

    Ok(())
}
