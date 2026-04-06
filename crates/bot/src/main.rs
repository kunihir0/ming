#![allow(clippy::module_name_repetitions)]

pub mod commands;
pub mod db;
pub mod services;

use anyhow::Context as _;
use db::DbPool;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use poise::serenity_prelude as serenity;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

#[derive(Clone)]
pub struct Data {
    pub db_pool: DbPool,
    pub push_receivers: Arc<Mutex<std::collections::HashMap<i32, tokio::task::JoinHandle<()>>>>,
    pub rustplus_clients: Arc<Mutex<std::collections::HashMap<i32, rustplus::RustPlusClient>>>,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let _ = dotenvy::dotenv();

    let _ = rustls::crypto::ring::default_provider().install_default();

    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://db.sqlite".to_string());
    let db_pool = db::establish_connection_pool(&database_url);

    // Run migrations
    {
        let mut conn = db_pool
            .get()
            .context("Failed to get DB connection from pool")?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Failed to run migrations: {e}"))?;
        info!("Database migrations applied.");
    }

    let token = env::var("DISCORD_TOKEN").context("Missing DISCORD_TOKEN env var")?;
    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    #[allow(clippy::collapsible_if)]
    let framework = poise::Framework::<Data, Error>::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::setup::setup(),
                commands::credentials::credentials(),
            ],
            event_handler: |ctx, event, _framework, data| {
                Box::pin(async move {
                    match event {
                        serenity::FullEvent::InteractionCreate { interaction } => {
                            if let Some(component_interaction) = interaction.as_message_component()
                            {
                                if let Err(e) = services::rustplus_client::handle_interaction(
                                    ctx,
                                    component_interaction,
                                    data,
                                )
                                .await
                                {
                                    tracing::error!("Error handling interaction: {e}");
                                }
                            }
                        }
                        serenity::FullEvent::Message { new_message } => {
                            if let Err(e) = services::rustplus_client::handle_discord_message(
                                ctx,
                                new_message,
                                data,
                            )
                            .await
                            {
                                tracing::error!("Error handling discord message: {e}");
                            }
                        }
                        _ => {}
                    }
                    Ok(())
                })
            },
            ..Default::default()
        })
        .setup({
            let db_pool = db_pool.clone();
            |ctx, _ready, framework| {
                Box::pin(async move {
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    info!("Bot is ready and commands are registered.");

                    let data = Data {
                        db_pool,
                        push_receivers: Arc::new(Mutex::new(std::collections::HashMap::new())),
                        rustplus_clients: Arc::new(Mutex::new(std::collections::HashMap::new())),
                    };

                    services::fcm::boot_existing_receivers(
                        &data.db_pool,
                        ctx.clone(),
                        data.push_receivers.clone(),
                    )
                    .await?;

                    services::rustplus_client::boot_existing_connections(&data, ctx.clone())
                        .await?;

                    Ok(data)
                })
            }
        })
        .build();

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await?;

    let shard_manager = client.shard_manager.clone();
    let db_pool_clone = db_pool.clone();
    let http_clone = client.http.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        info!("Ctrl+C received, shutting down gracefully...");

        let _ =
            services::dashboard::reset_all_dashboards_offline(&http_clone, &db_pool_clone).await;

        shard_manager.shutdown_all().await;
    });

    info!("Starting Discord bot...");
    client.start().await?;

    Ok(())
}
