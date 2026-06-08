use std::sync::Arc;
use std::time::Duration;
use db::DbPool;
use poise::serenity_prelude as serenity;
use tracing::{error, info};
use crate::tracking::battlemetrics::client::BmScraperClient;
use crate::tracking::steam::SteamService;
use std::collections::{HashMap, HashSet};

pub struct TrackerWatchdog {
    db_pool: DbPool,
    http: Arc<serenity::Http>,
    bm_client: BmScraperClient,
    steam_client: Arc<SteamService>,
}

impl TrackerWatchdog {
    pub fn new(db_pool: DbPool, http: Arc<serenity::Http>) -> Self {
        let steam_client = Arc::new(SteamService::new().unwrap());
        Self {
            db_pool,
            http,
            bm_client: BmScraperClient::new(),
            steam_client,
        }
    }

    pub async fn start(self: Arc<Self>) {
        info!("Starting Tracker Watchdog (3s interval)...");
        let mut interval = tokio::time::interval(Duration::from_secs(3));

        loop {
            interval.tick().await;
            if let Err(e) = self.run_cycle().await {
                error!("Error in Tracker Watchdog cycle: {}", e);
            }
        }
    }

    async fn run_cycle(&self) -> anyhow::Result<()> {
        let mut conn = self.db_pool.get()?;
        use db::schema::tracked_players::dsl::*;
        use db::schema::paired_servers::dsl as servers_dsl;
        use db::schema::player_name_history::dsl as pnh;
        use diesel::prelude::*;
        
        let all_paired_servers = servers_dsl::paired_servers.load::<db::models::PairedServer>(&mut conn)?;
        let mut servers_to_refresh = HashSet::new();

        for paired in all_paired_servers {
            let bm_server_id = match self.bm_client.scrape_server_id_by_ip(&paired.server_ip).await {
                Ok(Some(s_id)) => s_id,
                Ok(None) => {
                    tracing::warn!("Could not find BM Server ID for IP {}", paired.server_ip);
                    continue;
                }
                Err(e) => {
                    tracing::error!("Error scraping server ID by IP: {}", e);
                    continue;
                }
            };

            let bm_players = match self.bm_client.scrape_server_players(&bm_server_id).await {
                Ok(list) => list.players,
                Err(e) => {
                    tracing::error!("Failed to scrape BM server players for server {}: {}", bm_server_id, e);
                    continue;
                }
            };

            let mut online_bm_ids = HashMap::new();
            let mut online_names_to_bm_ids = HashMap::new();
            for p in &bm_players {
                online_bm_ids.insert(p.bm_id.clone(), p.name.clone());
                online_names_to_bm_ids.insert(p.name.clone(), p.bm_id.clone());
            }

            let server_players = tracked_players
                .filter(server_id.eq(paired.id))
                .load::<db::models::TrackedPlayer>(&mut conn)?;

            for player in server_players {
                let mut needs_refresh = false;

                if let Some(bm_id) = &player.bm_player_id {
                    let is_currently_online = online_bm_ids.contains_key(bm_id);
                    let is_online_db = player.is_online == 1;

                    if is_online_db != is_currently_online {
                        info!("Player {} ({}) status changed to: online={}", player.steam_id, bm_id, is_currently_online);
                        diesel::update(tracked_players.filter(id.eq(player.id)))
                            .set((
                                is_online.eq(if is_currently_online { 1 } else { 0 }),
                                last_known_server_id.eq(if is_currently_online { Some(bm_server_id.clone()) } else { None }),
                            ))
                            .execute(&mut conn)?;
                        needs_refresh = true;
                    }

                    if is_currently_online {
                        let current_name = online_bm_ids.get(bm_id).unwrap();
                        let name_changed = match &player.last_known_name {
                            Some(last_name) => last_name != current_name,
                            None => true,
                        };

                        if name_changed {
                            info!("Player {} ({}) changed name to {}", player.steam_id, bm_id, current_name);
                            diesel::update(tracked_players.filter(id.eq(player.id)))
                                .set(last_known_name.eq(current_name))
                                .execute(&mut conn)?;
                            
                            diesel::insert_into(pnh::player_name_history)
                                .values(db::models::NewPlayerNameHistory {
                                    tracked_player_id: player.id,
                                    name: current_name.clone(),
                                })
                                .execute(&mut conn)?;
                            needs_refresh = true;
                        }
                    }
                } else {
                    info!("Player {} has no BM ID linked. Attempting to cross-reference...", player.steam_id);
                    match self.steam_client.get_profile(&player.steam_id).await {
                        Ok(steam_profile) => {
                            let steam_name = steam_profile.persona_name;
                            info!("Steam Profile for {}: name = '{}'", player.steam_id, steam_name);
                            
                            if player.last_known_name.as_deref() != Some(&steam_name) {
                                diesel::update(tracked_players.filter(id.eq(player.id)))
                                    .set(last_known_name.eq(Some(&steam_name)))
                                    .execute(&mut conn)?;
                                needs_refresh = true;
                            }
                            
                            if let Some(matched_bm_id) = online_names_to_bm_ids.get(&steam_name) {
                                info!("✅ Cross-referenced Steam ID {} to BM ID {}", player.steam_id, matched_bm_id);
                                diesel::update(tracked_players.filter(id.eq(player.id)))
                                    .set((
                                        bm_player_id.eq(Some(matched_bm_id.to_string())),
                                        last_known_name.eq(Some(steam_name.clone())),
                                        is_online.eq(1),
                                        last_known_server_id.eq(Some(bm_server_id.clone())),
                                    ))
                                    .execute(&mut conn)?;
                                needs_refresh = true;
                            } else {
                                info!("Steam name '{}' not found in BM server player list.", steam_name);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to get Steam profile for {}: {}", player.steam_id, e);
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }

                if needs_refresh {
                    servers_to_refresh.insert(player.server_id);
                }
            }
        }
        
        for s_id in servers_to_refresh {
            if let Err(e) = crate::tracking::dashboard::refresh_dashboard(&self.http, &self.db_pool, s_id).await {
                error!("Failed to refresh dashboard for server {}: {}", s_id, e);
            }
        }
        
        Ok(())
    }
}
