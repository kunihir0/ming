use crate::{Context, Error};
use db::models::PairedServer;
use db::schema::fcm_credentials::dsl as fcm_dsl;
use db::schema::paired_servers::dsl as ps_dsl;
use diesel::prelude::*;
use std::fmt::Write as _;

/// Manage paired Rust servers
#[poise::command(
    slash_command,
    subcommands("list", "delete", "clear_all", "add_manual", "merge_rustplus"),
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

/// Add a BattleMetrics server manually for tracking (no Rust+ required)
#[poise::command(slash_command)]
pub async fn add_manual(
    ctx: Context<'_>,
    #[description = "BattleMetrics Server ID (e.g. 1234567)"] bm_server_id: String,
    #[description = "A short memorable name for this server"] name: String,
) -> Result<(), Error> {
    let guild_id_str = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();
    let mut conn = ctx.data().db_pool.get()?;

    // Find or create an FCM credential for this guild (just as a placeholder)
    let cred = fcm_dsl::fcm_credentials
        .filter(fcm_dsl::guild_id.eq(&guild_id_str))
        .first::<db::models::FcmCredential>(&mut conn)
        .optional()?;

    let fcm_id = match cred {
        Some(c) => c.id,
        None => {
            let new_cred = db::models::NewFcmCredential {
                guild_id: guild_id_str,
                gcm_android_id: "manual".to_string(),
                gcm_security_token: "manual".to_string(),
                steam_id: "0".to_string(),
                issued_date: 0,
                expire_date: 0,
            };
            diesel::insert_into(fcm_dsl::fcm_credentials)
                .values(&new_cred)
                .execute(&mut conn)?;

            fcm_dsl::fcm_credentials
                .order(db::schema::fcm_credentials::dsl::id.desc())
                .select(db::schema::fcm_credentials::dsl::id)
                .first::<i32>(&mut conn)?
        }
    };

    use rand::Rng;
    let random_token: i32 = rand::thread_rng().gen_range(1..100000);

    let new_server = db::models::NewPairedServer {
        fcm_credential_id: fcm_id,
        server_ip: "manual".to_string(),
        server_port: 0,
        player_token: random_token,
        name: name.clone(),
        auto_reconnect: 0,
        bm_server_id: Some(bm_server_id.clone()),
    };

    diesel::insert_into(ps_dsl::paired_servers)
        .values(&new_server)
        .execute(&mut conn)?;

    ctx.say(format!("✅ Successfully added manual tracking server **{}** (BM ID: {}). You can now use `/track setup_dashboard`.", name, bm_server_id)).await?;
    Ok(())
}

pub async fn autocomplete_server<'a>(
    ctx: Context<'a>,
    partial: &'a str,
) -> impl std::iter::Iterator<Item = poise::serenity_prelude::AutocompleteChoice> + 'a {
    let mut conn = match ctx.data().db_pool.get() {
        Ok(c) => c,
        Err(_) => return vec![].into_iter(),
    };

    let servers: Vec<PairedServer> = ps_dsl::paired_servers.load(&mut conn).unwrap_or_default();

    servers
        .into_iter()
        .filter(move |s| {
            partial.is_empty()
                || s.name.to_lowercase().contains(&partial.to_lowercase())
                || s.id.to_string().contains(partial)
        })
        .take(25)
        .map(|s| {
            poise::serenity_prelude::AutocompleteChoice::new(format!("{} (ID: {})", s.name, s.id), s.id as i64)
        })
        .collect::<Vec<_>>()
        .into_iter()
}

/// Merge a newly paired Rust+ server into an existing manual server
#[poise::command(slash_command)]
pub async fn merge_rustplus(
    ctx: Context<'_>,
    #[description = "The existing Manual Server ID"]
    #[autocomplete = "autocomplete_server"]
    manual_server_id: i32,
    #[description = "The newly paired Rust+ Server ID"]
    #[autocomplete = "autocomplete_server"]
    rustplus_server_id: i32,
) -> Result<(), Error> {
    let guild_id_str = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();
    let mut conn = ctx.data().db_pool.get()?;

    // Verify both servers belong to guild
    let manual_server: PairedServer = ps_dsl::paired_servers
        .inner_join(fcm_dsl::fcm_credentials)
        .filter(fcm_dsl::guild_id.eq(&guild_id_str))
        .filter(ps_dsl::id.eq(manual_server_id))
        .select(PairedServer::as_select())
        .first(&mut conn)
        .optional()?
        .ok_or("Manual server not found or doesn't belong to this guild.")?;

    let rustplus_server: PairedServer = ps_dsl::paired_servers
        .inner_join(fcm_dsl::fcm_credentials)
        .filter(fcm_dsl::guild_id.eq(&guild_id_str))
        .filter(ps_dsl::id.eq(rustplus_server_id))
        .select(PairedServer::as_select())
        .first(&mut conn)
        .optional()?
        .ok_or("Rust+ server not found or doesn't belong to this guild.")?;

    // Store properties
    let new_fcm_id = rustplus_server.fcm_credential_id;
    let new_ip = rustplus_server.server_ip.clone();
    let new_port = rustplus_server.server_port;
    let new_token = rustplus_server.player_token;

    ctx.say("Deleting Rust+ server record to merge into manual server...").await?;

    // Delete the new Rust+ server to free up the unique constraint
    let _ = crate::services::management::delete_server(
        ctx.serenity_context(),
        &ctx.data().db_pool,
        rustplus_server_id,
    )
    .await;

    // Update the manual server
    diesel::update(ps_dsl::paired_servers.filter(ps_dsl::id.eq(manual_server_id)))
        .set((
            ps_dsl::fcm_credential_id.eq(new_fcm_id),
            ps_dsl::server_ip.eq(new_ip),
            ps_dsl::server_port.eq(new_port),
            ps_dsl::player_token.eq(new_token),
        ))
        .execute(&mut conn)?;

    ctx.say(format!("✅ Successfully merged Rust+ connection into **{}**. Your tracking dashboard will now use live Rust+ data!", manual_server.name)).await?;
    Ok(())
}
