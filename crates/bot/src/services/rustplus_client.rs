use crate::Data;
use crate::db::models::{FcmCredential, PairedServer, ServerChannel, ServerSettings};
use crate::db::schema::{
    fcm_credentials::dsl as fcm_dsl, paired_servers::dsl as ps_dsl, server_channels::dsl as sc_dsl,
    server_settings::dsl as ss_dsl,
};
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use rustplus::RustPlusClient;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Handles interaction with connection/disconnection buttons.
///
/// # Errors
/// Returns an error if the database query fails or the Discord API call fails.
pub async fn handle_interaction(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
    data: &Data,
) -> anyhow::Result<()> {
    let custom_id = &interaction.data.custom_id;

    if custom_id.starts_with("connect_") || custom_id.starts_with("disconnect_") {
        let parts: Vec<&str> = custom_id.split('_').collect();
        if parts.len() != 2 {
            return Ok(());
        }

        let server_id: i32 = parts[1].parse()?;
        let is_connect = parts[0] == "connect";

        // Defer interaction
        interaction.defer(&ctx.http).await?;

        if is_connect {
            if let Err(e) = connect_server(server_id, data, ctx.clone()).await {
                error!("Failed to connect to server {}: {e}", server_id);
                interaction
                    .edit_response(
                        &ctx.http,
                        serenity::EditInteractionResponse::new()
                            .content(format!("Failed to connect: {e}")),
                    )
                    .await?;
            } else {
                interaction
                    .edit_response(
                        &ctx.http,
                        serenity::EditInteractionResponse::new().content("Connecting to server..."),
                    )
                    .await?;
            }
        } else {
            if let Err(e) = disconnect_server(server_id, data, ctx.clone()).await {
                error!("Failed to disconnect from server {}: {e}", server_id);
                interaction
                    .edit_response(
                        &ctx.http,
                        serenity::EditInteractionResponse::new()
                            .content(format!("Failed to disconnect: {e}")),
                    )
                    .await?;
            } else {
                interaction
                    .edit_response(
                        &ctx.http,
                        serenity::EditInteractionResponse::new()
                            .content("Disconnected from server."),
                    )
                    .await?;
            }
        }
    }

    Ok(())
}

/// Automatically reconnects previously active Rust+ servers on startup.
///
/// # Errors
/// Returns an error if the database query fails or resetting dashboards fails.
pub async fn boot_existing_connections(data: &Data, ctx: serenity::Context) -> anyhow::Result<()> {
    // 1. Reset all dashboards to offline initially to clear stale "Online" states
    let _ =
        crate::services::dashboard::reset_all_dashboards_offline(&ctx.http, &data.db_pool).await;

    let mut conn = data.db_pool.get()?;
    let servers: Vec<PairedServer> = ps_dsl::paired_servers
        .filter(ps_dsl::auto_reconnect.eq(1))
        .load(&mut conn)?;

    for server in servers {
        let server_id = server.id;
        let data_clone = data.clone();
        let ctx_clone = ctx.clone();
        tokio::spawn(async move {
            if let Err(e) = connect_server(server_id, &data_clone, ctx_clone).await {
                error!("Failed to auto-reconnect to server {}: {}", server_id, e);
            }
        });
    }

    Ok(())
}

/// Forwards messages from Discord to the Rust team chat queue.
///
/// # Errors
/// Returns an error if the database query fails.
pub async fn handle_discord_message(
    _ctx: &serenity::Context,
    msg: &serenity::Message,
    data: &Data,
) -> anyhow::Result<()> {
    if msg.author.bot {
        return Ok(());
    }

    let mut conn = data.db_pool.get()?;
    let channel_id_str = msg.channel_id.get().to_string();

    let server_channel: Option<ServerChannel> = sc_dsl::server_channels
        .filter(sc_dsl::chat_channel_id.eq(&channel_id_str))
        .first::<ServerChannel>(&mut conn)
        .optional()?;

    if let Some(sc) = server_channel {
        let settings: ServerSettings = ss_dsl::server_settings
            .find(sc.server_id)
            .first(&mut conn)?;

        if settings.bridge_discord_to_rust == 0 {
            return Ok(());
        }

        let queues = data.chat_queues.lock().await;
        if let Some(tx) = queues.get(&sc.server_id) {
            let rust_msg = format!("[Discord] {}: {}", msg.author.name, msg.content);
            let _ = tx.send(rust_msg).await;
        }
    }

    Ok(())
}

/// Connects to a Rust+ server and starts background listeners.
///
/// # Errors
/// Returns an error if connection fails or database operations fail.
#[allow(clippy::too_many_lines)]
pub async fn connect_server(
    server_id: i32,
    data: &Data,
    ctx: serenity::Context,
) -> anyhow::Result<()> {
    let mut conn = data.db_pool.get()?;

    let server: PairedServer = ps_dsl::paired_servers.find(server_id).first(&mut conn)?;

    // Set auto_reconnect to 1 if not already set
    if server.auto_reconnect != 1 {
        diesel::update(ps_dsl::paired_servers.find(server_id))
            .set(ps_dsl::auto_reconnect.eq(1))
            .execute(&mut conn)?;
    }

    // Immediately update dashboard to "Connecting"
    let _ = crate::services::dashboard::update_dashboard_online(
        &ctx.http,
        &data.db_pool,
        server_id,
        None,
        None,
    )
    .await;

    let cred: FcmCredential = fcm_dsl::fcm_credentials
        .find(server.fcm_credential_id)
        .first(&mut conn)?;

    let server_channel: Option<ServerChannel> = sc_dsl::server_channels
        .find(server_id)
        .first::<ServerChannel>(&mut conn)
        .optional()?;

    let mut lock = data.rustplus_clients.lock().await;

    if lock.contains_key(&server_id) {
        return Err(anyhow::anyhow!("Already connected or connecting."));
    }

    let steam_id = cred.steam_id.parse::<u64>()?;

    let mut client = RustPlusClient::new(
        server.server_ip.clone(),
        u16::try_from(server.server_port).unwrap_or(28082),
        steam_id,
        server.player_token,
        false, // Try direct first
    );

    match client.connect().await {
        Ok(()) => {
            info!(
                "Connected to Rust+ server {} ({}:{}) directly.",
                server.name, server.server_ip, server.server_port
            );
        }
        Err(e) => {
            warn!(
                "Failed to connect directly to {}, retrying with Facepunch proxy... Error: {}",
                server.name, e
            );
            // Retry with proxy
            client = RustPlusClient::new(
                server.server_ip.clone(),
                u16::try_from(server.server_port).unwrap_or(28082),
                steam_id,
                server.player_token,
                true, // Use proxy
            );
            client.connect().await?;
            info!(
                "Connected to Rust+ server {} ({}:{}) via Facepunch proxy.",
                server.name, server.server_ip, server.server_port
            );
        }
    }

    let Some(mut rx) = client.take_broadcast_receiver() else {
        return Err(anyhow::anyhow!("Failed to acquire broadcast receiver"));
    };

    lock.insert(server_id, client.clone());

    // Monitor Loop & Event Router setup
    let (event_tx, event_rx) = tokio::sync::broadcast::channel::<rustplus::events::RustEvent>(100);
    let mut monitor_loop = rustplus::monitor::MonitorLoop::new(client.clone(), event_tx);
    monitor_loop.register(Box::new(rustplus::monitors::cargo::CargoMonitor::new()));
    monitor_loop.register(Box::new(rustplus::monitors::vending::VendingMonitor::new()));
    tokio::spawn(monitor_loop.run());

    // Fetch settings for event toggles
    let settings: ServerSettings = {
        let mut conn = data.db_pool.get()?;
        ss_dsl::server_settings
            .find(server_id)
            .first::<ServerSettings>(&mut conn)?
    };

    let mut event_config = std::collections::HashMap::new();
    #[allow(clippy::collapsible_if)]
    if let Some(alerts_channel) = server_channel
        .as_ref()
        .and_then(|sc| sc.alerts_channel_id.clone())
    {
        if let Ok(ch_id) = alerts_channel.parse::<u64>() {
            let ch = serenity::ChannelId::new(ch_id);
            if settings.events_cargo == 1 {
                event_config.insert(crate::services::events::EventKind::Cargo, ch);
            }
            if settings.events_heli == 1 {
                event_config.insert(crate::services::events::EventKind::Heli, ch);
            }
            if settings.events_oilrig == 1 {
                event_config.insert(crate::services::events::EventKind::OilRig, ch);
            }
            if settings.events_ch47 == 1 {
                event_config.insert(crate::services::events::EventKind::Ch47, ch);
            }
            if settings.events_vending == 1 {
                event_config.insert(crate::services::events::EventKind::VendingMachine, ch);
            }
        }
    }

    let notifier = std::sync::Arc::new(crate::services::events::Notifier::new(
        ctx.http.clone(),
        event_config,
    ));

    // Setup Chat Queue early so we can pass the sender to the router
    let (tx, mut chat_rx) = mpsc::channel::<String>(100);
    data.chat_queues.lock().await.insert(server_id, tx.clone());

    let router = crate::services::events::EventRouter::new(
        server_id,
        notifier,
        data.sub_store.clone(),
        Some(tx),
    );
    tokio::spawn(router.run(event_rx));

    // Fetch initial server and team info
    let server_info = match client.get_info().await {
        Ok(msg) => msg.response.and_then(|r| r.info),
        Err(e) => {
            warn!("Failed to fetch initial server info: {}", e);
            None
        }
    };

    let team_info = match client.get_team_info().await {
        Ok(msg) => msg.response.and_then(|r| r.team_info),
        Err(e) => {
            warn!("Failed to fetch initial team info: {}", e);
            None
        }
    };

    // Update Dashboard to Online
    if let Err(e) = crate::services::dashboard::update_dashboard_online(
        &ctx.http,
        &data.db_pool,
        server_id,
        server_info.as_ref(),
        team_info.as_ref(),
    )
    .await
    {
        error!("Failed to update dashboard to online: {}", e);
    }

    // Spawn Chat Dispatcher Task
    let client_clone = client.clone();
    let db_pool_clone = data.db_pool.clone();
    tokio::spawn(async move {
        while let Some(msg) = chat_rx.recv().await {
            let cooldown = {
                let Ok(mut conn) = db_pool_clone.get() else {
                    break;
                };
                match ss_dsl::server_settings
                    .find(server_id)
                    .select(ss_dsl::chat_cooldown)
                    .first::<i32>(&mut conn)
                {
                    Ok(c) => c,
                    Err(_) => 0,
                }
            };

            if let Err(e) = client_clone.send_team_message(&msg).await {
                error!(
                    "Failed to send team message to Rust server {}: {}",
                    server_id, e
                );
            }

            if cooldown > 0 {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    u64::try_from(cooldown).unwrap_or(0),
                ))
                .await;
            }
        }
    });

    // Spawn a background task to listen to the receiver
    let clients_arc = data.rustplus_clients.clone();
    let queues_arc = data.chat_queues.clone();
    let pool_clone = data.db_pool.clone();
    let ctx_clone = ctx.clone();
    let server_name = server.name.clone();
    let rust_server_ip = server.server_ip.clone();
    let server_port = server.server_port;
    let chat_channel_id = server_channel
        .as_ref()
        .and_then(|sc| sc.chat_channel_id.clone());
    let data_clone = data.clone();
    let client_clone = client.clone();

    // Cache current info for updates
    let current_server_info = server_info;

    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Some(broadcast) = &msg.broadcast {
                // Handle Team Changed
                if let Some(team_changed) = &broadcast.team_changed {
                    let ti = &team_changed.team_info;
                    if let Some(si) = &current_server_info {
                        let _ = crate::services::dashboard::update_dashboard_online(
                            &ctx_clone.http,
                            &pool_clone,
                            server_id,
                            Some(si),
                            Some(ti),
                        )
                        .await;
                    }
                }

                // Handle Team Message
                if let Some(team_msg) = &broadcast.team_message {
                    let message_text = team_msg.message.message.clone();
                    let sender_name = team_msg.message.name.clone();

                    info!(
                        "[{}] Team Chat: {}: {}",
                        server_name, sender_name, message_text
                    );

                    let settings: ServerSettings = {
                        let Ok(mut conn) = pool_clone.get() else {
                            break;
                        };
                        match ss_dsl::server_settings
                            .find(server_id)
                            .first::<ServerSettings>(&mut conn)
                        {
                            Ok(s) => s,
                            Err(_) => break,
                        }
                    };

                    // Process In-Game Commands
                    let data_clone = data_clone.clone();
                    let client_clone = client_clone.clone();
                    let server_ip_clone = rust_server_ip.clone();
                    let message_text_clone = message_text.clone();
                    let settings_clone = settings.clone();
                    tokio::spawn(async move {
                        if let Ok(Some(response)) = data_clone
                            .gcommands
                            .handle_message(
                                &message_text_clone,
                                server_id,
                                &server_ip_clone,
                                server_port,
                                &settings_clone,
                                &data_clone,
                            )
                            .await
                        {
                            let _ = client_clone.send_team_message(&response).await;
                        }
                    });

                    #[allow(clippy::collapsible_if)]
                    if settings.bridge_rust_to_discord == 1
                        && !message_text.starts_with(&settings.in_game_prefix)
                        && !message_text.starts_with("[Discord]")
                    {
                        if let Some(channel_id) = chat_channel_id
                            .as_ref()
                            .and_then(|id| id.parse::<u64>().ok())
                        {
                            let channel = serenity::ChannelId::new(channel_id);
                            let discord_msg = format!("**[{sender_name}]**: {message_text}");
                            let _ = channel.say(&ctx_clone.http, discord_msg).await;
                        }
                    }
                }
            }
        }

        warn!("Rust+ connection to {} lost.", server_name);
        clients_arc.lock().await.remove(&server_id);
        queues_arc.lock().await.remove(&server_id);

        // Update Dashboard to Offline
        let _ = crate::services::dashboard::update_dashboard_offline(
            &ctx_clone.http,
            &pool_clone,
            server_id,
        )
        .await;
    });

    Ok(())
}

/// Disconnects from a Rust+ server and resets its dashboard.
///
/// # Errors
/// Returns an error if database operations fail.
pub async fn disconnect_server(
    server_id: i32,
    data: &Data,
    ctx: serenity::Context,
) -> anyhow::Result<()> {
    let mut lock = data.rustplus_clients.lock().await;

    if let Some(mut client) = lock.remove(&server_id) {
        client.disconnect();
        info!("Disconnected from server {}", server_id);
    }

    data.chat_queues.lock().await.remove(&server_id);

    // Always reset auto_reconnect to 0
    let mut conn = data.db_pool.get()?;
    diesel::update(ps_dsl::paired_servers.find(server_id))
        .set(ps_dsl::auto_reconnect.eq(0))
        .execute(&mut conn)?;

    let _ =
        crate::services::dashboard::update_dashboard_offline(&ctx.http, &data.db_pool, server_id)
            .await;

    Ok(())
}
