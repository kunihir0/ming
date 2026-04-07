#![allow(clippy::module_name_repetitions)]

pub mod commands;
pub mod db;
pub mod gcommands;
pub mod services;
pub mod utils;

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
    pub chat_queues: Arc<Mutex<std::collections::HashMap<i32, tokio::sync::mpsc::Sender<String>>>>,
    pub battlemetrics: Arc<services::battlemetrics::BattlemetricsService>,
    pub gcommands: Arc<gcommands::GCommandRegistry>,
    pub map_service: Arc<services::map::MapService>,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("bot=debug,rustplus=debug,push_receiver=info,serenity=info")
        .init();
    let _ = dotenvy::dotenv();

    let _ = rustls::crypto::ring::default_provider().install_default();

    let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let db_pool = db::establish_connection_pool(&database_url);

    {
        let mut conn = db_pool.get()?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Migration error: {e}"))?;
        info!("Database migrations applied.");
    }

    let token = env::var("DISCORD_TOKEN").context("Missing DISCORD_TOKEN")?;
    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_MESSAGES;

    let framework = poise::Framework::<Data, Error>::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::setup::setup(),
                commands::credentials::credentials(),
                commands::servers::servers(),
            ],
            event_handler: |ctx, event, _framework, data| {
                Box::pin(async move {
                    match event {
                        serenity::FullEvent::InteractionCreate { interaction } => match interaction
                        {
                            serenity::Interaction::Component(component) => {
                                if let Err(e) = services::rustplus_client::handle_interaction(
                                    ctx, component, data,
                                )
                                .await
                                {
                                    tracing::error!("Error handling interaction: {e}");
                                }
                                if let Err(e) =
                                    services::config_dashboard::handle_config_interaction(
                                        ctx, component, data,
                                    )
                                    .await
                                {
                                    tracing::error!("Error handling config interaction: {e}");
                                }
                                if let Err(e) = services::management::handle_pairing_interaction(
                                    ctx, component, data,
                                )
                                .await
                                {
                                    tracing::error!("Error handling pairing interaction: {e}");
                                }
                            }
                            serenity::Interaction::Modal(modal) => {
                                if let Err(e) = services::config_dashboard::handle_modal_submit(
                                    ctx, modal, data,
                                )
                                .await
                                {
                                    tracing::error!("Error handling config modal: {e}");
                                }
                            }
                            _ => {}
                        },
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
                        chat_queues: Arc::new(Mutex::new(std::collections::HashMap::new())),
                        battlemetrics: Arc::new(
                            services::battlemetrics::BattlemetricsService::new(),
                        ),
                        gcommands: Arc::new(gcommands::GCommandRegistry::new()),
                        map_service: Arc::new(services::map::MapService::new()),
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
