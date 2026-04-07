use crate::db::models::PairedServer;
use crate::db::schema::fcm_credentials::dsl as fcm_dsl;
use crate::db::schema::paired_servers::dsl as ps_dsl;
use crate::{Context, Error};
use diesel::prelude::*;
use std::fmt::Write as _;

/// Manage paired Rust servers
#[poise::command(
    slash_command,
    subcommands("list", "delete", "clear_all"),
    required_permissions = "ADMINISTRATOR"
)]
#[allow(clippy::unused_async)]
pub async fn servers(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// List all paired servers in this guild
#[poise::command(slash_command)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id_str = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();
    let mut conn = ctx.data().db_pool.get()?;

    let servers: Vec<PairedServer> = ps_dsl::paired_servers
        .inner_join(fcm_dsl::fcm_credentials)
        .filter(fcm_dsl::guild_id.eq(&guild_id_str))
        .select(PairedServer::as_select())
        .load(&mut conn)?;

    if servers.is_empty() {
        ctx.say("No servers are currently paired in this guild.")
            .await?;
        return Ok(());
    }

    let mut response = "**Paired Servers:**\n".to_string();
    for server in servers {
        let _ = writeln!(
            response,
            "- **{}** (ID: {}) - `{}:{}`",
            server.name, server.id, server.server_ip, server.server_port
        );
    }

    ctx.say(response).await?;
    Ok(())
}

/// Delete a paired server and its Discord channels
#[poise::command(slash_command)]
pub async fn delete(
    ctx: Context<'_>,
    #[description = "Server ID from /servers list"] id: i32,
) -> Result<(), Error> {
    let guild_id_str = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();
    let mut conn = ctx.data().db_pool.get()?;

    // Verify server belongs to guild
    let server: Option<PairedServer> = ps_dsl::paired_servers
        .inner_join(fcm_dsl::fcm_credentials)
        .filter(ps_dsl::id.eq(id))
        .filter(fcm_dsl::guild_id.eq(&guild_id_str))
        .select(PairedServer::as_select())
        .first(&mut conn)
        .optional()?;

    let Some(server) = server else {
        ctx.say(format!(
            "Server with ID {id} not found or doesn't belong to this guild."
        ))
        .await?;
        return Ok(());
    };

    ctx.say(format!(
        "Deleting server **{}** and its channels...",
        server.name
    ))
    .await?;

    crate::services::management::delete_server(ctx.serenity_context(), &ctx.data().db_pool, id)
        .await?;

    ctx.say("Server deleted successfully.").await?;
    Ok(())
}

/// Clear all paired servers and their Discord channels
#[poise::command(slash_command)]
pub async fn clear_all(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id_str = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();
    let mut conn = ctx.data().db_pool.get()?;

    // Get all servers for guild
    let servers: Vec<i32> = ps_dsl::paired_servers
        .inner_join(fcm_dsl::fcm_credentials)
        .filter(fcm_dsl::guild_id.eq(&guild_id_str))
        .select(ps_dsl::id)
        .load(&mut conn)?;

    if servers.is_empty() {
        ctx.say("No servers to clear.").await?;
        return Ok(());
    }

    ctx.say(format!(
        "Clearing {} servers and their channels...",
        servers.len()
    ))
    .await?;

    for id in servers {
        let _ = crate::services::management::delete_server(
            ctx.serenity_context(),
            &ctx.data().db_pool,
            id,
        )
        .await;
    }

    ctx.say("All servers cleared successfully.").await?;
    Ok(())
}
