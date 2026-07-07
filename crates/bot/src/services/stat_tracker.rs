use db::{DbPool, models::NewPlayerStat, schema::player_stats};
use diesel::prelude::*;
use rustplus::events::RustEvent;
use rustplus::proto::AppMarker;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tracing::{error, info};

pub struct StatTracker {
    server_id: i32,
    db_pool: DbPool,
    last_positions: Mutex<HashMap<String, PlayerState>>,
}

struct PlayerState {
    x: f32,
    y: f32,
    last_moved: std::time::Instant,
    is_afk: bool,
}

impl StatTracker {
    #[must_use]
    pub fn new(server_id: i32, db_pool: DbPool) -> Self {
        Self {
            server_id,
            db_pool,
            last_positions: Mutex::new(HashMap::new()),
        }
    }

    pub async fn run(self: Arc<Self>, mut rx: broadcast::Receiver<RustEvent>) {
        while let Ok(event) = rx.recv().await {
            match event {
                RustEvent::MarkerSnapshot(markers) => {
                    self.process_markers(markers).await;
                }
                _ => {} // Ignore other events including RawBroadcast
            }
        }
    }

    async fn process_markers(&self, markers: Vec<AppMarker>) {
        let mut states = self.last_positions.lock().await;
        for marker in markers {
            if marker.r#type() != rustplus::proto::AppMarkerType::Player {
                continue;
            }

            let steam_id = marker.steam_id.unwrap_or_default().to_string();
            if steam_id == "0" || steam_id.is_empty() {
                continue;
            }

            let current_pos = (marker.x, marker.y);

            if let Some(state) = states.get_mut(&steam_id) {
                let dx = state.x - current_pos.0;
                let dy = state.y - current_pos.1;
                let dist = (dx * dx + dy * dy).sqrt();

                // Impossible distance in 1 tick (assuming snapshot is ~5-10 seconds)
                // A player running at 5.5m/s for 10s = 55m. Let's say 200m is teleport/death.
                if dist > 200.0 {
                    self.log_event(&steam_id, "Death", state.x, state.y);
                    state.x = current_pos.0;
                    state.y = current_pos.1;
                    state.last_moved = std::time::Instant::now();
                    if state.is_afk {
                        state.is_afk = false;
                        self.log_event(&steam_id, "AfkEnd", current_pos.0, current_pos.1);
                    }
                    continue;
                }

                if dist < 1.0 {
                    // Hasn't moved
                    if !state.is_afk
                        && state.last_moved.elapsed() > std::time::Duration::from_secs(300)
                    {
                        state.is_afk = true;
                        self.log_event(&steam_id, "AfkStart", current_pos.0, current_pos.1);
                    }
                } else {
                    state.x = current_pos.0;
                    state.y = current_pos.1;
                    state.last_moved = std::time::Instant::now();
                    if state.is_afk {
                        state.is_afk = false;
                        self.log_event(&steam_id, "AfkEnd", current_pos.0, current_pos.1);
                    }
                }
            } else {
                states.insert(
                    steam_id.clone(),
                    PlayerState {
                        x: current_pos.0,
                        y: current_pos.1,
                        last_moved: std::time::Instant::now(),
                        is_afk: false,
                    },
                );
                self.log_event(&steam_id, "Online", current_pos.0, current_pos.1);
            }
        }
    }

    fn log_event(&self, steam_id: &str, event: &str, x: f32, y: f32) {
        let mut conn = match self.db_pool.get() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to get DB connection to log stat: {}", e);
                return;
            }
        };

        let new_stat = NewPlayerStat {
            server_id: self.server_id,
            steam_id: steam_id.to_string(),
            event_type: event.to_string(),
            x,
            y,
        };

        if let Err(e) = diesel::insert_into(player_stats::table)
            .values(&new_stat)
            .execute(&mut conn)
        {
            error!("Failed to insert player stat: {}", e);
        } else {
            info!(
                "Logged stat for {}: {} at {:.0}, {:.0}",
                steam_id, event, x, y
            );
        }
    }
}
