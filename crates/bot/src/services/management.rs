use crate::Data;
use crate::db::DbPool;
use crate::db::models::{PairedServer, PairingRequest, ServerChannel};
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use tracing::{error, info};

/// Handles interaction with pairing request buttons (Approve/Ignore).
///
/// # Errors
/// Returns an error if the database query fails or Discord API calls fail.
pub async fn handle_pairing_interaction(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
    data: &Data,
) -> anyhow::Result<()> {
    use crate::db::schema::pairing_requests::dsl as pr_dsl;

    let custom_id = &interaction.data.custom_id;

    if custom_id.starts_with("pair_approve_") || custom_id.starts_with("pair_ignore_") {
        let is_approve = custom_id.starts_with("pair_approve_");
        let request_id = custom_id.split('_').next_back().unwrap_or_default();

        let mut conn = data.db_pool.get()?;

        let request: Option<PairingRequest> = pr_dsl::pairing_requests
            .find(request_id)
            .first::<PairingRequest>(&mut conn)
            .optional()?;

        let Some(req) = request else {
            interaction
                .create_response(
                    &ctx.http,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("Request not found or already processed.")
                            .ephemeral(true),
                    ),
                )
                .await?;
            return Ok(());
        };

        if is_approve {
            // Check if already paired
            use crate::db::schema::paired_servers::dsl as ps_dsl;
            let existing: Option<PairedServer> = ps_dsl::paired_servers
                .filter(ps_dsl::server_ip.eq(&req.server_ip))
                .filter(ps_dsl::server_port.eq(req.server_port))
                .filter(ps_dsl::player_token.eq(req.player_token))
                .first::<PairedServer>(&mut conn)
                .optional()?;

            if existing.is_some() {
                interaction
                    .create_response(
                        &ctx.http,
                        serenity::CreateInteractionResponse::Message(
                            serenity::CreateInteractionResponseMessage::new()
                                .content("This server is already paired.")
                                .ephemeral(true),
                        ),
                    )
                    .await?;
                diesel::delete(pr_dsl::pairing_requests.find(request_id)).execute(&mut conn)?;
                return Ok(());
            }

            let new_server = crate::db::models::NewPairedServer {
                fcm_credential_id: req.fcm_credential_id,
                server_ip: req.server_ip.clone(),
                server_port: req.server_port,
                player_token: req.player_token,
                name: req.name.clone(),
                auto_reconnect: 1,
            };

            diesel::insert_into(ps_dsl::paired_servers)
                .values(&new_server)
                .execute(&mut conn)?;

            // SQLite doesn't support RETURNING, fetch it back
            let inserted_server: PairedServer = ps_dsl::paired_servers
                .filter(ps_dsl::server_ip.eq(&req.server_ip))
                .filter(ps_dsl::server_port.eq(req.server_port))
                .filter(ps_dsl::player_token.eq(req.player_token))
                .first::<PairedServer>(&mut conn)?;

            // Setup Dashboard
            if let Err(e) = crate::services::dashboard::handle_new_paired_server(
                &data.db_pool,
                ctx,
                &req.guild_id,
                &inserted_server,
            )
            .await
            {
                error!("Failed to setup dashboard for approved server: {e}");
            }

            interaction
                .create_response(
                    &ctx.http,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content(format!("✅ Approved pairing for **{}**", req.name))
                            .ephemeral(false),
                    ),
                )
                .await?;
        } else {
            interaction
                .create_response(
                    &ctx.http,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content(format!("❌ Ignored pairing request for **{}**", req.name))
                            .ephemeral(false),
                    ),
                )
                .await?;
        }

        // Delete request from DB
        diesel::delete(pr_dsl::pairing_requests.find(request_id)).execute(&mut conn)?;

        // Delete the original request message
        let _ = interaction.message.delete(&ctx.http).await;
    }

    Ok(())
}

/// Deletes a paired server and all its associated Discord resources.
///
/// # Errors
/// Returns an error if database operations or Discord API calls fail.
pub async fn delete_server(
    ctx: &serenity::Context,
    db_pool: &DbPool,
    server_id: i32,
) -> anyhow::Result<()> {
    use crate::db::schema::paired_servers::dsl as ps_dsl;
    use crate::db::schema::server_channels::dsl as sc_dsl;

    let mut conn = db_pool.get()?;

    let server: PairedServer = ps_dsl::paired_servers.find(server_id).first(&mut conn)?;
    let channels: Option<ServerChannel> = sc_dsl::server_channels
        .find(server_id)
        .first::<ServerChannel>(&mut conn)
        .optional()?;

    info!("Deleting server: {} (ID: {})", server.name, server_id);

    // 1. Cleanup Discord
    if let Some(sc) = channels {
        let channel_ids = vec![
            sc.dashboard_channel_id,
            sc.chat_channel_id,
            sc.alerts_channel_id,
            sc.config_channel_id,
        ];

        for id_str in channel_ids.into_iter().flatten() {
            if let Ok(id) = id_str.parse::<u64>() {
                let _ = serenity::ChannelId::new(id).delete(&ctx.http).await;
            }
        }

        if let Some(cat_id_str) = sc.category_id {
            #[allow(clippy::collapsible_if)]
            if let Ok(id) = cat_id_str.parse::<u64>() {
                let _ = serenity::ChannelId::new(id).delete(&ctx.http).await;
            }
        }
    }

    // 2. Cleanup Database (Cascades to server_channels, server_settings)
    diesel::delete(ps_dsl::paired_servers.find(server_id)).execute(&mut conn)?;

    Ok(())
}
