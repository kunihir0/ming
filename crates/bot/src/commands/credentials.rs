use crate::db::models::NewFcmCredential;
use crate::{Context, Error};
use diesel::prelude::*;

/// Manage FCM credentials
#[allow(clippy::unused_async)]
#[poise::command(
    slash_command,
    subcommands("add"),
    required_permissions = "ADMINISTRATOR"
)]
pub async fn credentials(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Add new FCM credentials
#[allow(clippy::items_after_statements)]
#[poise::command(slash_command)]
pub async fn add(
    ctx: Context<'_>,
    #[description = "GCM Android ID"] gcm_android_id: String,
    #[description = "GCM Security Token"] gcm_security_token: String,
    #[description = "Steam ID"] steam_id: String,
    #[description = "Issued Date"] issued_date: i64,
    #[description = "Expire Date"] expire_date: i64,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();

    let new_cred = NewFcmCredential {
        guild_id: guild_id.clone(),
        gcm_android_id,
        gcm_security_token,
        steam_id,
        issued_date,
        expire_date,
    };

    use crate::db::schema::fcm_credentials::dsl::fcm_credentials;
    let mut conn = ctx.data().db_pool.get()?;

    // Check if guild_configs exists first
    use crate::db::models::GuildConfig;
    use crate::db::schema::guild_configs::dsl::guild_configs;
    let config_exists = guild_configs
        .find(&guild_id)
        .first::<GuildConfig>(&mut conn)
        .is_ok();

    if !config_exists {
        ctx.say("You must run `/setup` first before adding credentials!")
            .await?;
        return Ok(());
    }

    diesel::insert_into(fcm_credentials)
        .values(&new_cred)
        .execute(&mut conn)?;

    // Fetch the newly created credential
    use crate::db::models::FcmCredential;
    let cred = fcm_credentials
        .order(crate::db::schema::fcm_credentials::dsl::id.desc())
        .first::<FcmCredential>(&mut conn)?;

    // Start the FCM listener
    let handle = crate::services::fcm::start_listener(
        cred.clone(),
        ctx.data().db_pool.clone(),
        ctx.serenity_context().clone(),
    );
    ctx.data()
        .push_receivers
        .lock()
        .await
        .insert(cred.id, handle);

    ctx.say("Credentials added successfully!").await?;

    Ok(())
}
