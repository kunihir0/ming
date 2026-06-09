mod autocomplete;
mod connection_manager;
mod fcm;
mod framework;
mod items;
mod listener;
mod vending;
pub mod tracking;

use crate::connection_manager::ConnectionManager;
use crate::framework::{CommandRegistry, MinibotData, ReplyTarget, UnifiedContext};
use crate::tracking::commands::{track, TrackCommand};
use crate::vending::{VendingListCommand, VendingSearchCommand, VendingSubsCommand, VendingDumpCommand};
use anyhow::Context as _;
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use std::collections::HashMap;
use std::env;
use std::fmt::Write as _;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

type Error = Box<dyn std::error::Error + Send + Sync>;
type PoiseContext<'a> = poise::Context<'a, Arc<MinibotData>, Error>;

#[derive(Debug, poise::ChoiceParameter)]
pub enum SearchType {
    #[name = "Buy (Acquire this item)"]
    Buy,
    #[name = "Sell (Trade away this item)"]
    Sell,
}

// ---------------------------------------------------------------------------
// Vending commands: /v search | /v subs | /v list | /v dump
// ---------------------------------------------------------------------------

#[poise::command(slash_command, subcommands("search", "subs", "list", "dump"), subcommand_required, category = "Vending Machines")]
async fn v(_ctx: PoiseContext<'_>) -> Result<(), Error> {
    Ok(())
}

/// Show the bot's help menu
#[poise::command(slash_command, category = "Settings")]
async fn help(ctx: PoiseContext<'_>) -> Result<(), Error> {
    let mut help_text = String::from("```asciidoc\n= Minibot Command Reference =\n");

    let commands = &ctx.framework().options().commands;
    let mut categories: std::collections::BTreeMap<&str, Vec<_>> = std::collections::BTreeMap::new();
    
    for cmd in commands {
        if cmd.hide_in_help { continue; }
        let cat = cmd.category.as_deref().unwrap_or("Uncategorized");
        categories.entry(cat).or_default().push(cmd);
    }
    
    for (cat_name, cmds) in categories {
        help_text.push_str(&format!("\n== {} ==\n", cat_name));
        for cmd in cmds {
            if cmd.subcommands.is_empty() {
                let desc = cmd.description.as_deref().unwrap_or("No description");
                help_text.push_str(&format!("/{:<20} :: {}\n", cmd.name, desc));
            } else {
                for sub in &cmd.subcommands {
                    if sub.hide_in_help { continue; }
                    let desc = sub.description.as_deref().unwrap_or("No description");
                    let full_name = format!("{} {}", cmd.name, sub.name);
                    help_text.push_str(&format!("/{:<20} :: {}\n", full_name, desc));
                }
            }
        }
    }
    
    help_text.push_str("```");
    
    ctx.send(poise::CreateReply::default().content(help_text).ephemeral(true)).await?;
    Ok(())
}

/// Smart search for items in vending machines (supports regex/wildcards)
#[poise::command(slash_command)]
async fn search(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
    #[description = "Search type (default: Buy)"]
    search_type: Option<SearchType>,
    #[description = "Item 1"]
    #[autocomplete = "crate::autocomplete::autocomplete_item"]
    query1: String,
    #[description = "Item 2"]
    #[autocomplete = "crate::autocomplete::autocomplete_item"]
    query2: Option<String>,
    #[description = "Item 3"]
    #[autocomplete = "crate::autocomplete::autocomplete_item"]
    query3: Option<String>,
    #[description = "Item 4"]
    #[autocomplete = "crate::autocomplete::autocomplete_item"]
    query4: Option<String>,
    #[description = "Item 5"]
    #[autocomplete = "crate::autocomplete::autocomplete_item"]
    query5: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let uctx = discord_context(ctx.data(), server_id, ctx.author().id.get(), ctx.channel_id());
    let cmd = VendingSearchCommand;
    
    let type_str = match search_type {
        Some(SearchType::Sell) => "sell",
        _ => "buy",
    };

    let mut all_queries = vec![query1];
    if let Some(q) = query2 { all_queries.push(q); }
    if let Some(q) = query3 { all_queries.push(q); }
    if let Some(q) = query4 { all_queries.push(q); }
    if let Some(q) = query5 { all_queries.push(q); }
    let combined_query = all_queries.join(", ");

    let args = [type_str, combined_query.as_str()];
    ctx.say(format!("Searching for {}...", combined_query)).await?;
    match crate::framework::UnifiedCommand::execute(&cmd, &uctx, &args).await {
        Ok(response) => {
            let pages = response.pages;
            if pages.len() == 1 {
                let mut embed = poise::serenity_prelude::CreateEmbed::new()
                    .description(&pages[0])
                    .color(0xCE422B);
                if let Some(thumb) = response.thumbnail_url {
                    embed = embed.thumbnail(thumb);
                }
                ctx.send(poise::CreateReply::default().embed(embed)).await?;
            } else if pages.len() > 1 {
                let page_refs: Vec<&str> = pages.iter().map(|s| s.as_str()).collect();
                poise::builtins::paginate(ctx, &page_refs).await?;
            }
        }
        Err(e) => {
            ctx.say(format!("Error: {}", e)).await?;
        }
    }
    Ok(())
}

/// Subscribe to a vending machine item
#[poise::command(slash_command)]
async fn subs(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
    #[description = "Item query"]
    #[autocomplete = "crate::autocomplete::autocomplete_item"]
    query: String,
) -> Result<(), Error> {
    let uctx = discord_context(ctx.data(), server_id, ctx.author().id.get(), ctx.channel_id());
    let cmd = VendingSubsCommand;
    let args: Vec<&str> = query.split_whitespace().collect();

    match crate::framework::UnifiedCommand::execute(&cmd, &uctx, &args).await {
        Ok(response) => {
            let pages = response.pages;
            if pages.len() == 1 {
                ctx.say(&pages[0]).await?;
            } else if pages.len() > 1 {
                let page_refs: Vec<&str> = pages.iter().map(|s| s.as_str()).collect();
                poise::builtins::paginate(ctx, &page_refs).await?;
            }
        }
        Err(e) => {
            ctx.say(format!("Error: {}", e)).await?;
        }
    }
    Ok(())
}

/// List active vending machine subscriptions
#[poise::command(slash_command)]
async fn list(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
) -> Result<(), Error> {
    ctx.defer().await?;
    let uctx = discord_context(ctx.data(), server_id, ctx.author().id.get(), ctx.channel_id());
    let cmd = VendingListCommand;
    let args: Vec<&str> = vec![];

    match crate::framework::UnifiedCommand::execute(&cmd, &uctx, &args).await {
        Ok(response) => {
            let pages = response.pages;
            if pages.len() == 1 {
                ctx.say(&pages[0]).await?;
            } else if pages.len() > 1 {
                let page_refs: Vec<&str> = pages.iter().map(|s| s.as_str()).collect();
                poise::builtins::paginate(ctx, &page_refs).await?;
            }
        }
        Err(e) => {
            ctx.say(format!("Error: {}", e)).await?;
        }
    }
    Ok(())
}

/// Dump the entire server's vending machine list to a JSON file (sent via DM)
#[poise::command(slash_command)]
async fn dump(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
) -> Result<(), Error> {
    ctx.defer().await?;
    let uctx = discord_context(ctx.data(), server_id, ctx.author().id.get(), ctx.channel_id());
    let cmd = VendingDumpCommand;
    let args: Vec<&str> = vec![];

    match crate::framework::UnifiedCommand::execute(&cmd, &uctx, &args).await {
        Ok(response) => {
            let pages = response.pages;
            if pages.len() == 1 {
                ctx.say(&pages[0]).await?;
            } else if pages.len() > 1 {
                let page_refs: Vec<&str> = pages.iter().map(|s| s.as_str()).collect();
                poise::builtins::paginate(ctx, &page_refs).await?;
            }
        }
        Err(e) => {
            ctx.say(format!("Error: {}", e)).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Server management commands: /server connect | disconnect | list
// ---------------------------------------------------------------------------

/// Manage Rust+ server connections
#[poise::command(
    slash_command,
    subcommands("server_connect", "server_disconnect", "server_list", "server_clear_all"),
    subcommand_required,
    category = "Server Management",
    required_permissions = "MANAGE_GUILD"
)]
async fn server(_ctx: PoiseContext<'_>) -> Result<(), Error> {
    Ok(())
}

/// Connect to a paired Rust+ server
#[poise::command(slash_command, rename = "connect")]
async fn server_connect(
    ctx: PoiseContext<'_>,
    #[description = "Server ID from /server list"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
) -> Result<(), Error> {
    ctx.defer().await?;
    let conn_mgr = ctx.data().connection_manager.lock().await;
    match conn_mgr.as_ref() {
        Some(mgr) => match mgr.connect(server_id).await {
            Ok(()) => {
                ctx.say(format!("✅ Connected to server {}", server_id))
                    .await?;
            }
            Err(e) => {
                ctx.say(format!("❌ Failed to connect: {}", e)).await?;
            }
        },
        None => {
            ctx.say("Connection manager not ready yet.").await?;
        }
    }
    Ok(())
}

/// Disconnect from a Rust+ server
#[poise::command(slash_command, rename = "disconnect")]
async fn server_disconnect(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
) -> Result<(), Error> {
    let conn_mgr = ctx.data().connection_manager.lock().await;
    match conn_mgr.as_ref() {
        Some(mgr) => match mgr.disconnect(server_id).await {
            Ok(()) => {
                ctx.say(format!("Disconnected from server {}", server_id))
                    .await?;
            }
            Err(e) => {
                ctx.say(format!("❌ Failed to disconnect: {}", e)).await?;
            }
        },
        None => {
            ctx.say("Connection manager not ready yet.").await?;
        }
    }
    Ok(())
}

/// Clear all server connections and delete from DB
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD", rename = "clear_all")]
async fn server_clear_all(
    ctx: PoiseContext<'_>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let mut conn_mgr = ctx.data().connection_manager.lock().await;
    if let Some(mgr) = conn_mgr.as_mut() {
        mgr.clear_all().await?;
        ctx.say("All paired servers have been disconnected and deleted. You can now re-pair in game.").await?;
    } else {
        ctx.say("Connection manager not ready yet.").await?;
    }
    Ok(())
}

/// List all paired servers and their connection status
#[poise::command(slash_command, rename = "list")]
async fn server_list(ctx: PoiseContext<'_>) -> Result<(), Error> {
    let conn_mgr = ctx.data().connection_manager.lock().await;
    match conn_mgr.as_ref() {
        Some(mgr) => {
            let servers = mgr.list_servers().await?;
            if servers.is_empty() {
                ctx.say("No paired servers.").await?;
            } else {
                let mut response = String::from("**Paired Servers:**\n");
                for (srv, connected) in &servers {
                    let status = if *connected { "🟢" } else { "⚫" };
                    let _ = writeln!(
                        response,
                        "{} **{}** (ID: {}) — `{}:{}`",
                        status, srv.name, srv.id, srv.server_ip, srv.server_port
                    );
                }
                ctx.say(response).await?;
            }
        }
        None => {
            ctx.say("Connection manager not ready yet.").await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Config commands
// ---------------------------------------------------------------------------

/// Set the default Discord channel for in-game replies
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD", category = "Config")]
async fn set_reply_channel(
    ctx: PoiseContext<'_>,
    #[description = "Server ID"]
    #[autocomplete = "crate::autocomplete::autocomplete_server"]
    server_id: i32,
    #[description = "Channel for in-game replies"] channel: serenity::Channel,
) -> Result<(), Error> {
    let mut map = ctx.data().reply_channels.lock().await;
    map.insert(server_id, channel.id());
    ctx.say(format!(
        "In-game replies for server {} will now be sent to <#{}>",
        server_id,
        channel.id()
    ))
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Credentials commands: /credentials add | /credentials list
// ---------------------------------------------------------------------------

/// Manage FCM credentials for Rust+ pairing
#[poise::command(
    slash_command,
    subcommands("creds_add", "creds_list", "creds_clear_all"),
    subcommand_required,
    category = "Credentials",
    required_permissions = "ADMINISTRATOR"
)]
async fn credentials(_ctx: PoiseContext<'_>) -> Result<(), Error> {
    Ok(())
}

/// Add new FCM credentials
#[poise::command(slash_command, rename = "add")]
async fn creds_add(
    ctx: PoiseContext<'_>,
    #[description = "GCM Android ID"] gcm_android_id: String,
    #[description = "GCM Security Token"] gcm_security_token: String,
    #[description = "Steam ID"] steam_id: String,
    #[description = "Issued Date"] issued_date: i64,
    #[description = "Expire Date"] expire_date: i64,
) -> Result<(), Error> {
    let gid = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();
    let mut conn = ctx.data().db_pool.get()?;

    // Ensure guild_configs row exists (FK requirement)
    {
        use db::schema::guild_configs::dsl::*;
        let exists = guild_configs
            .find(&gid)
            .first::<db::models::GuildConfig>(&mut conn)
            .is_ok();

        if !exists {
            diesel::insert_into(guild_configs)
                .values(db::models::GuildConfig {
                    guild_id: gid.clone(),
                    setup_mode: "Auto".to_string(),
                    manual_dashboard_channel_id: None,
                    manual_chat_channel_id: None,
                    manual_alerts_channel_id: None,
                    manual_cctv_channel_id: None,
                    manual_ai_channel_id: None,
                    in_game_prefix: "@".to_string(),
                    management_channel_id: None,
                })
                .execute(&mut conn)?;
        }
    }

    let new_cred = db::models::NewFcmCredential {
        guild_id: gid,
        gcm_android_id,
        gcm_security_token,
        steam_id,
        issued_date,
        expire_date,
    };

    diesel::insert_into(db::schema::fcm_credentials::dsl::fcm_credentials)
        .values(&new_cred)
        .execute(&mut conn)?;

    // Fetch the newly created credential to get its id
    let cred: db::models::FcmCredential = db::schema::fcm_credentials::dsl::fcm_credentials
        .order(db::schema::fcm_credentials::dsl::id.desc())
        .first(&mut conn)?;

    // Start an FCM listener for this credential immediately
    let conn_mgr_lock = ctx.data().connection_manager.lock().await;
    if let Some(conn_mgr) = conn_mgr_lock.as_ref() {
        let handle = crate::fcm::spawn_single_listener(
            cred.clone(),
            ctx.data().db_pool.clone(),
            conn_mgr.clone(),
        );
        // Fire and forget — the JoinHandle runs in the background
        drop(handle);
    }

    ctx.say(format!(
        "✅ Credentials added (ID: {}). FCM listener started — servers will auto-pair on connect.",
        cred.id
    ))
    .await?;
    Ok(())
}

/// List all stored FCM credentials
#[poise::command(slash_command, rename = "list")]
async fn creds_list(ctx: PoiseContext<'_>) -> Result<(), Error> {
    let gid = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();
    let mut conn = ctx.data().db_pool.get()?;

    use db::schema::fcm_credentials::dsl::*;
    let creds: Vec<db::models::FcmCredential> = fcm_credentials
        .filter(guild_id.eq(&gid))
        .load(&mut conn)?;

    if creds.is_empty() {
        ctx.say("No credentials stored. Use `/credentials add` to add some.")
            .await?;
        return Ok(());
    }

    let mut response = String::from("**Stored Credentials:**\n");
    for c in &creds {
        let _ = writeln!(
            response,
            "- ID: {} | Steam: `{}` | Expires: `{}`",
            c.id, c.steam_id, c.expire_date
        );
    }
    
    ctx.say(response).await?;
    Ok(())
}

/// Clear all FCM credentials
#[poise::command(slash_command, rename = "clear_all")]
async fn creds_clear_all(ctx: PoiseContext<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let gid = ctx.guild_id().ok_or("Must be run in a guild")?.to_string();
    
    // Disconnect and clear all paired servers first
    let mut conn_mgr = ctx.data().connection_manager.lock().await;
    if let Some(mgr) = conn_mgr.as_mut() {
        let _ = mgr.clear_all().await;
    }
    
    let mut conn = ctx.data().db_pool.get()?;
    use db::schema::fcm_credentials::dsl::*;
    use diesel::prelude::*;
    
    // Delete all credentials for this guild
    diesel::delete(fcm_credentials.filter(guild_id.eq(&gid))).execute(&mut conn)?;
    
    ctx.say("✅ All FCM credentials and paired servers have been wiped. You can now start completely fresh!").await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn discord_context<'a>(
    data: &'a MinibotData,
    server_id: i32,
    author_id: u64,
    channel_id: serenity::ChannelId,
) -> UnifiedContext<'a> {
    UnifiedContext {
        server_id,
        data,
        reply_target: ReplyTarget::Discord { channel_id },
        discord_id: Some(author_id.to_string()),
        steam_id: None,
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("minibot=debug,songbird=debug,info")
        .init();
    let _ = dotenvy::dotenv();
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let db_pool = db::establish_connection_pool(&database_url);

    {
        let mut conn = db_pool.get()?;
        db::run_migrations(&mut conn)?;
        info!("Database migrations applied.");
    }

    let token = env::var("DISCORD_TOKEN").context("Missing DISCORD_TOKEN")?;
    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT | serenity::GatewayIntents::GUILD_VOICE_STATES;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![v(), server(), credentials(), set_reply_channel(), track()],
            event_handler: |ctx, event, _framework, data| {
                Box::pin(async move {
                    if let poise::serenity_prelude::FullEvent::InteractionCreate { interaction } = event {
                        if let poise::serenity_prelude::Interaction::Component(component) = interaction {
                            let custom_id = component.data.custom_id.clone();
                            if custom_id.starts_with("track_") {
                                if let Err(e) = crate::tracking::dashboard::handle_component(ctx, component, &data.db_pool).await {
                                    tracing::error!("Dashboard component error: {}", e);
                                }
                            }
                        } else if let poise::serenity_prelude::Interaction::Modal(modal) = interaction {
                            let custom_id = modal.data.custom_id.clone();
                            if custom_id.starts_with("track_") {
                                if let Err(e) = crate::tracking::dashboard::handle_modal(ctx, modal, &data.db_pool).await {
                                    tracing::error!("Dashboard modal error: {}", e);
                                }
                            }
                        }
                    }
                    Ok(())
                })
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            let db_pool = db_pool.clone();
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                info!("Commands registered.");

                let mut registry = CommandRegistry::new();
                registry.register(VendingSearchCommand);
                registry.register(VendingSubsCommand);
                registry.register(VendingListCommand);
                registry.register(VendingDumpCommand);
                registry.register(TrackCommand);
                let registry = Arc::new(registry);

                let data = Arc::new(MinibotData {
                    db_pool: db_pool.clone(),
                    rustplus_clients: Arc::new(Mutex::new(HashMap::new())),
                    reply_channels: Arc::new(Mutex::new(HashMap::new())),
                    discord_http: ctx.http.clone(),
                    connection_manager: Arc::new(Mutex::new(None)),
                });

                // Build ConnectionManager with a reference to shared state
                let conn_mgr = Arc::new(ConnectionManager::new(
                    db_pool.clone(),
                    registry,
                    data.clone(),
                ));

                // Store it so Discord commands can access it
                {
                    let mut lock = data.connection_manager.lock().await;
                    *lock = Some(conn_mgr.clone());
                }

                // Boot existing connections
                conn_mgr.boot().await;
                conn_mgr.clone().start_watchdog();

                // Start tracking watchdog
                let songbird_manager = songbird::get(ctx).await.expect("Songbird Voice client placed in at initialization.");
                let tracking_watchdog = Arc::new(crate::tracking::watchdog::TrackerWatchdog::new(db_pool.clone(), ctx.http.clone(), songbird_manager));
                tokio::spawn(tracking_watchdog.start());

                // Start FCM listeners for auto-pairing
                let _fcm_handles = match fcm::boot_fcm_listeners(&db_pool, conn_mgr).await {
                    Ok(h) => h,
                    Err(e) => {
                        error!("Failed to boot FCM listeners: {}", e);
                        vec![]
                    }
                };

                Ok(data)
            })
        })
        .build();

    use songbird::SerenityInit;
    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .register_songbird()
        .await
        .unwrap();

    info!("Starting Minibot...");
    client.start().await?;

    Ok(())
}
