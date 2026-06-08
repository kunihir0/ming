use serenity::builder::{CreateModal, CreateActionRow, CreateInputText, CreateInteractionResponse, CreateInteractionResponseMessage};
use poise::serenity_prelude as serenity;
use anyhow::Result;
use db::DbPool;
use diesel::prelude::*;

pub async fn handle_component(ctx: &serenity::Context, component: &serenity::ComponentInteraction, db_pool: &DbPool) -> Result<()> {
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
        _ => {}
    }
    
    let _ = modal.create_followup(&ctx.http, CreateInteractionResponseMessage::new().content(success_msg).ephemeral(true)).await;
    
    // Refresh the dashboard
    let _ = crate::tracking::dashboard::refresh_dashboard(&ctx.http, db_pool, server_id).await;
    
    Ok(())
}
