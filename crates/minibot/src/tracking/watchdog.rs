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
    songbird_manager: Arc<songbird::Songbird>,
    reqwest_client: reqwest::Client,
}

impl TrackerWatchdog {
    pub fn new(db_pool: DbPool, http: Arc<serenity::Http>, songbird_manager: Arc<songbird::Songbird>) -> Self {
        let steam_client = Arc::new(SteamService::new().unwrap());
        Self {
            db_pool,
            http,
            bm_client: BmScraperClient::new(),
            steam_client,
            songbird_manager,
            reqwest_client: reqwest::Client::new(),
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

                        // Check TTS config
                        use db::schema::track_notifications_config::dsl as tnc_dsl;
                        if let Ok(Some(config)) = tnc_dsl::track_notifications_config
                            .filter(tnc_dsl::server_id.eq(paired.id))
                            .first::<db::models::TrackNotificationsConfig>(&mut conn)
                            .optional() 
                        {
                            if let Some(vc_id_str) = config.tts_voice_channel_id {
                                let should_alert = if is_currently_online { config.alert_on_join == 1 } else { config.alert_on_leave == 1 };
                                if should_alert && config.tts_enabled == 1 {
                                    use db::schema::fcm_credentials::dsl as fcm_dsl;
                                    use db::schema::paired_servers::dsl as ps_dsl;
                                    
                                    // Need to find guild_id to join VC
                                    if let Ok(Some(cred)) = fcm_dsl::fcm_credentials
                                        .inner_join(ps_dsl::paired_servers)
                                        .filter(ps_dsl::id.eq(paired.id))
                                        .select(fcm_dsl::fcm_credentials::all_columns())
                                        .first::<db::models::FcmCredential>(&mut conn)
                                        .optional()
                                    {
                                        if let (Ok(guild_id), Ok(vc_id)) = (cred.guild_id.parse::<u64>(), vc_id_str.parse::<u64>()) {
                                            let action = if is_currently_online { "joined" } else { "left" };
                                            let name_to_say = player.last_known_name.clone().unwrap_or_else(|| "Someone".to_string());
                                            let tts_text = format!("{} {}", name_to_say, action);
                                            
                                            let songbird_manager = self.songbird_manager.clone();
                                            let reqwest_client = self.reqwest_client.clone();
                                            let http = self.http.clone();
                                            
                                            tokio::spawn(async move {
                                                let channel_id = serenity::model::id::ChannelId::new(vc_id);
                                                let actual_guild_id = match http.get_channel(channel_id).await {
                                                    Ok(serenity::model::channel::Channel::Guild(gc)) => gc.guild_id,
                                                    _ => serenity::model::id::GuildId::new(guild_id), // fallback
                                                };
                                                
                                                match crate::tracking::tts::generate_tts(&tts_text, "en_us_001", &reqwest_client).await {
                                                    Ok(bytes) => {
                                                        if let Err(e) = crate::tracking::tts::play_and_leave(
                                                            songbird_manager,
                                                            actual_guild_id,
                                                            channel_id,
                                                            bytes
                                                        ).await {
                                                            error!("Failed to play TTS: {}", e);
                                                        }
                                                    }
                                                    Err(e) => error!("Failed to generate TTS: {}", e),
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
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
