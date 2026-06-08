use anyhow::{Context as _, Result};
use db::models::{FcmCredential, PairedServer};
use db::DbPool;
use rustplus::RustPlusClient;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::framework::CommandRegistry;

/// Manages Rust+ server connections backed by the database.
pub struct ConnectionManager {
    pub clients: Arc<Mutex<HashMap<i32, RustPlusClient>>>,
    db_pool: DbPool,
    registry: Arc<CommandRegistry>,
    data_ref: Arc<crate::framework::MinibotData>,
}

impl ConnectionManager {
    pub fn new(
        db_pool: DbPool,
        registry: Arc<CommandRegistry>,
        data_ref: Arc<crate::framework::MinibotData>,
    ) -> Self {
        Self {
            clients: data_ref.rustplus_clients.clone(),
            db_pool,
            registry,
            data_ref,
        }
    }

    /// Connects all servers marked with `auto_reconnect = 1`.
    pub async fn boot(&self) {
        let servers = {
            let mut conn = match self.db_pool.get() {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to get DB connection for boot: {}", e);
                    return;
                }
            };
            use db::schema::paired_servers::dsl::*;
            use diesel::prelude::*;
            paired_servers
                .filter(auto_reconnect.eq(1))
                .load::<PairedServer>(&mut conn)
                .unwrap_or_default()
        };

        for server in servers {
            let sid = server.id;
            if let Err(e) = self.connect(sid).await {
                error!("Failed to auto-connect server {}: {}", sid, e);
            }
        }
    }

    /// Spawns a background watchdog that loops and reconnects disconnected servers that have auto_reconnect=1.
    pub fn start_watchdog(self: Arc<Self>) {
        let mgr = self;
        tokio::spawn(async move {
            info!("Starting ConnectionManager watchdog...");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                
                let servers = {
                    let mut conn = match mgr.db_pool.get() {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Watchdog DB error: {}", e);
                            continue;
                        }
                    };
                    use db::schema::paired_servers::dsl::*;
                    use diesel::prelude::*;
                    paired_servers
                        .filter(auto_reconnect.eq(1))
                        .load::<PairedServer>(&mut conn)
                        .unwrap_or_default()
                };

                let active_clients = {
                    let lock = mgr.clients.lock().await;
                    lock.keys().copied().collect::<Vec<i32>>()
                };

                for server in servers {
                    if !active_clients.contains(&server.id) {
                        warn!("Watchdog detecting disconnected server {}. Reconnecting...", server.id);
                        if let Err(e) = mgr.connect(server.id).await {
                            error!("Watchdog reconnect failed for {}: {}", server.id, e);
                        }
                    }
                }
            }
        });
    }

    /// Connects to a specific server by its DB id.
    pub async fn connect(&self, server_id: i32) -> Result<()> {
        let (server, cred) = {
            let mut conn = self.db_pool.get().context("DB pool error")?;
            use db::schema::fcm_credentials::dsl as fcm_dsl;
            use db::schema::paired_servers::dsl as ps_dsl;
            use diesel::prelude::*;

            let server: PairedServer = ps_dsl::paired_servers
                .find(server_id)
                .first(&mut conn)
                .context("Server not found in database")?;

            let cred: FcmCredential = fcm_dsl::fcm_credentials
                .find(server.fcm_credential_id)
                .first(&mut conn)
                .context("FCM credential not found")?;

            (server, cred)
        };

        {
            let lock = self.clients.lock().await;
            if lock.contains_key(&server_id) {
                anyhow::bail!("Already connected to server {}", server_id);
            }
        }

        let steam_id = cred.steam_id.parse::<u64>().context("Invalid steam_id")?;
        let port = u16::try_from(server.server_port).unwrap_or(28082);

        // Try direct first, then proxy
        let mut client = RustPlusClient::new(
            server.server_ip.clone(),
            port,
            steam_id,
            server.player_token,
            false,
        );

        match client.connect().await {
            Ok(()) => {
                info!(
                    "Connected to {} ({}:{}) directly",
                    server.name, server.server_ip, server.server_port
                );
            }
            Err(e) => {
                warn!(
                    "Direct connect to {} failed ({}), retrying via proxy...",
                    server.name, e
                );
                client = RustPlusClient::new(
                    server.server_ip.clone(),
                    port,
                    steam_id,
                    server.player_token,
                    true,
                );
                client.connect().await.context("Proxy connect also failed")?;
                info!(
                    "Connected to {} ({}:{}) via proxy",
                    server.name, server.server_ip, server.server_port
                );
            }
        }

        // Subscribe to team chat so the server actually sends us messages!
        if let Err(e) = client.get_team_chat().await {
            client.disconnect();
            anyhow::bail!("Failed to subscribe to team chat: {}", e);
        }

        // Mark auto_reconnect
        {
            let mut conn = self.db_pool.get()?;
            use db::schema::paired_servers::dsl::*;
            use diesel::prelude::*;
            diesel::update(paired_servers.find(server_id))
                .set(auto_reconnect.eq(1))
                .execute(&mut conn)?;
        }

        // Take broadcast receiver before inserting into the map
        let rx = client.take_broadcast_receiver();
        self.clients.lock().await.insert(server_id, client);

        // Spawn the in-game listener for this server
        if let Some(rx) = rx {
            let data = self.data_ref.clone();
            let registry = self.registry.clone();
            let clients_arc = self.clients.clone();
            let _db_pool = self.db_pool.clone();
            let server_name = server.name.clone();

            tokio::spawn(async move {
                crate::listener::run_in_game_listener(server_id, data, registry, rx).await;

                // Connection dropped — clean up
                warn!("Connection to {} (id={}) lost", server_name, server_id);
                clients_arc.lock().await.remove(&server_id);
            });
        }

        Ok(())
    }

    /// Disconnects from a specific server.
    pub async fn disconnect(&self, server_id: i32) -> Result<()> {
        let removed = {
            let mut lock = self.clients.lock().await;
            lock.remove(&server_id)
        };

        if let Some(mut client) = removed {
            client.disconnect();
            info!("Disconnected from server {}", server_id);
        } else {
            anyhow::bail!("Not connected to server {}", server_id);
        }

        // Clear auto_reconnect
        let mut conn = self.db_pool.get()?;
        use db::schema::paired_servers::dsl::*;
        use diesel::prelude::*;
        diesel::update(paired_servers.find(server_id))
            .set(auto_reconnect.eq(0))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Disconnects from all servers and deletes them from the database.
    pub async fn clear_all(&self) -> Result<()> {
        {
            let mut lock = self.clients.lock().await;
            for (_, mut client) in lock.drain() {
                client.disconnect();
            }
        }
        
        let mut conn = self.db_pool.get()?;
        use db::schema::paired_servers::dsl::*;
        use diesel::prelude::*;
        diesel::delete(paired_servers).execute(&mut conn)?;

        Ok(())
    }

    /// Returns a list of all paired servers and their connection status.
    pub async fn list_servers(&self) -> Result<Vec<(PairedServer, bool)>> {
        let mut conn = self.db_pool.get()?;
        use db::schema::paired_servers::dsl::*;
        use diesel::prelude::*;

        let servers = paired_servers.load::<PairedServer>(&mut conn)?;
        let lock = self.clients.lock().await;

        Ok(servers
            .into_iter()
            .map(|s| {
                let connected = lock.contains_key(&s.id);
                (s, connected)
            })
            .collect())
    }
}
