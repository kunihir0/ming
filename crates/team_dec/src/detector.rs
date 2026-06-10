use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::error::Result;
use crate::models::{ConnectionData, GraphData, GraphEdge, GraphNode, Player};
use crate::services::battlemetrics::BattleMetricsService;
use crate::services::steam::SteamService;
use crate::services::steamid_com::SteamIdDotComService;

#[derive(Debug, Clone)]
pub struct TeamDetectorConfig {
    pub debug: bool,
    pub recursive_depth: u32,
    pub search_comments: bool,
    pub search_comments_max_pages: u32,
    pub ignore_list: HashSet<String>,
    pub include_offline: bool,
    pub max_profiles: u32,
}

impl Default for TeamDetectorConfig {
    fn default() -> Self {
        Self {
            debug: false,
            recursive_depth: 5,
            search_comments: false,
            search_comments_max_pages: 1,
            ignore_list: HashSet::new(),
            include_offline: false,
            max_profiles: 200,
        }
    }
}

#[derive(Default)]
pub struct TeamDetectorBuilder {
    config: TeamDetectorConfig,
}


impl TeamDetectorBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn debug(mut self, debug: bool) -> Self {
        self.config.debug = debug;
        self
    }

    pub fn recursive_depth(mut self, depth: u32) -> Self {
        self.config.recursive_depth = depth.clamp(1, 10);
        self
    }

    pub fn include_offline(mut self, include_offline: bool) -> Self {
        self.config.include_offline = include_offline;
        self
    }

    pub fn max_profiles(mut self, max: u32) -> Self {
        self.config.max_profiles = max;
        self
    }

    pub fn ignore_list(mut self, list: Vec<String>) -> Self {
        self.config.ignore_list = list.into_iter().collect();
        self
    }

    pub fn build(self) -> TeamDetector {
        TeamDetector::new(self.config)
    }
}

pub struct TeamDetector {
    config: TeamDetectorConfig,
    steam: Arc<SteamService>,
    bm: Arc<BattleMetricsService>,
    steamid_com: Arc<SteamIdDotComService>,
}

impl TeamDetector {
    pub fn new(config: TeamDetectorConfig) -> Self {
        let debug = config.debug;
        Self {
            config,
            steam: Arc::new(SteamService::new(debug)),
            bm: Arc::new(BattleMetricsService::new(debug)),
            steamid_com: Arc::new(SteamIdDotComService::new(debug)),
        }
    }

    fn log(&self, msg: &str) {
        if self.config.debug {
            println!("[TeamDetector] {}", msg);
        }
    }

    pub async fn run(
        &self,
        server_id: &str,
        seed_steam_ids: Vec<String>,
    ) -> Result<(Vec<Player>, GraphData)> {
        self.log(&format!(
            "Starting search on server {} with {} seeds",
            server_id,
            seed_steam_ids.len()
        ));

        let bm_players = self.bm.get_players(server_id).await.unwrap_or_default();
        let mut found_players = Vec::new();
        let mut searched_steam_ids = HashSet::new();
        let mut profiles_searched = 0;
        let mut peoples_connections: HashMap<String, ConnectionData> = HashMap::new();
        let mut nodes: HashMap<String, GraphNode> = HashMap::new();
        let mut edges: Vec<GraphEdge> = Vec::new();

        // Iterative BFS queue: (steam_id, depth)
        let mut queue: std::collections::VecDeque<(String, u32)> =
            std::collections::VecDeque::new();
        for id in seed_steam_ids {
            queue.push_back((id, 0));
        }

        while let Some((profile_steam_id, current_depth)) = queue.pop_front() {
            if self.config.ignore_list.contains(&profile_steam_id) {
                self.log(&format!("Skipping ignored: {}", profile_steam_id));
                continue;
            }

            if current_depth >= self.config.recursive_depth {
                continue;
            }

            if profiles_searched >= self.config.max_profiles {
                self.log("Max profiles reached. Stopping search.");
                break;
            }

            if searched_steam_ids.contains(&profile_steam_id) {
                continue;
            }

            profiles_searched += 1;
            searched_steam_ids.insert(profile_steam_id.clone());
            println!(
                "[{}/{}] Searching profile: {} (depth {})",
                profiles_searched, self.config.max_profiles, profile_steam_id, current_depth
            );

            let profile_name = self
                .steam
                .get_profile_name(&profile_steam_id)
                .await
                .unwrap_or_default();
            let profile_custom_id = self
                .steam
                .get_custom_id_by_steam_id(&profile_steam_id)
                .await
                .ok();
            let profile_status = self
                .steam
                .get_profile_status(&profile_steam_id)
                .await
                .unwrap_or_else(|_| "Offline".to_string());
            let is_on_server = bm_players.contains(&profile_name);

            found_players.push(Player {
                steam_id: Some(profile_steam_id.clone()),
                custom_id: profile_custom_id.clone(),
                name: profile_name.clone(),
                status: Some(profile_status.clone()),
                is_on_server: Some(is_on_server),
                source_type: None,
            });

            nodes.insert(
                profile_name.clone(),
                GraphNode {
                    id: profile_name.clone(),
                    label: profile_name.clone(),
                    steam_id: Some(profile_steam_id.clone()),
                    custom_id: profile_custom_id.clone(),
                    status: Some(profile_status),
                    is_on_server: Some(is_on_server),
                },
            );

            let mut people = Vec::new();

            // 1. Steam Friends
            if let Ok(friends) = self.steam.get_friends(&profile_steam_id).await {
                people.extend(friends);
            }

            // 2. Hidden Friends via steamid.com
            if let Ok(hidden_friends) = self.steamid_com.get_friends(&profile_steam_id).await {
                for hf in hidden_friends {
                    // Only take depth 1 to match normal friends behavior
                    if hf.depth == 1 {
                        people.push(Player {
                            steam_id: Some(hf.steam_id64),
                            custom_id: None,
                            name: hf.persona_name,
                            status: None,
                            is_on_server: None,
                            source_type: Some("hidden_friends".to_string()),
                        });
                    }
                }
            }

            // 3. Comments (Optional)
            if self.config.search_comments && self.config.search_comments_max_pages > 0
                && let Ok(num_comments) = self.steam.get_number_of_comments(&profile_steam_id).await {
                    let mut remaining_comments = num_comments;
                    for i in 1..=self.config.search_comments_max_pages {
                        if remaining_comments == 0 {
                            break;
                        }
                        if let Ok((read, authors)) = self
                            .steam
                            .get_comments_page_authors(&profile_steam_id, i)
                            .await
                        {
                            remaining_comments = remaining_comments.saturating_sub(read);
                            people.extend(authors);
                        }
                    }
                }

            // Deduplicate
            people = Self::remove_duplicates(people);
            people.retain(|p| p.steam_id.as_ref() != Some(&profile_steam_id));
            if let Some(ref pc) = profile_custom_id {
                people.retain(|p| p.custom_id.as_ref() != Some(pc));
            }

            // Identify BM presence
            for p in &mut people {
                p.is_on_server = Some(bm_players.contains(&p.name));
            }

            // Edges & Nodes
            for p in &people {
                if !nodes.contains_key(&p.name) {
                    nodes.insert(
                        p.name.clone(),
                        GraphNode {
                            id: p.name.clone(),
                            label: p.name.clone(),
                            steam_id: p.steam_id.clone(),
                            custom_id: p.custom_id.clone(),
                            status: None,
                            is_on_server: p.is_on_server,
                        },
                    );
                }

                if self.config.include_offline || p.is_on_server == Some(true) {
                    edges.push(GraphEdge {
                        from: profile_name.clone(),
                        to: p.name.clone(),
                    });
                }
            }

            peoples_connections.insert(
                profile_steam_id.clone(),
                ConnectionData {
                    name: profile_name.clone(),
                    custom_id: profile_custom_id.clone(),
                    connections: people.clone(),
                },
            );

            // Filter for recursion
            let mut recursion_targets = people;
            if !self.config.include_offline {
                recursion_targets.retain(|p| p.is_on_server == Some(true));
            }

            for t in recursion_targets {
                let mut target_steam_id = t.steam_id.clone();
                if target_steam_id.is_none()
                    && let Some(ref custom_id) = t.custom_id {
                        target_steam_id = self.steam.get_steam_id_by_custom_id(custom_id).await.ok();
                    }

                if let Some(sid) = target_steam_id {
                    let already_found = found_players
                        .iter()
                        .any(|f| f.steam_id.as_ref() == Some(&sid));
                    if !already_found {
                        queue.push_back((sid, current_depth + 1));
                    }
                }
            }
        }

        // Post-processing connections
        let conns: Vec<_> = peoples_connections.values().collect();
        for outer in &conns {
            for inner in &conns {
                if outer.name == inner.name {
                    continue;
                }
                if outer.custom_id.is_some() && outer.custom_id == inner.custom_id {
                    continue;
                }

                let outer_in_inner = inner.connections.iter().any(|c| {
                    (c.name == outer.name)
                        || (c.custom_id.is_some() && c.custom_id == outer.custom_id)
                });

                if outer_in_inner {
                    edges.push(GraphEdge {
                        from: outer.name.clone(),
                        to: inner.name.clone(),
                    });
                }
            }
        }

        let graph = GraphData {
            nodes: nodes.into_values().collect(),
            edges,
        };

        Ok((found_players, graph))
    }

    fn remove_duplicates(people: Vec<Player>) -> Vec<Player> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();

        for p in people {
            let key = if let Some(sid) = &p.steam_id {
                format!("s:{}", sid)
            } else if let Some(cid) = &p.custom_id {
                format!("c:{}", cid)
            } else {
                format!("n:{}", p.name)
            };

            if seen.insert(key) {
                out.push(p);
            }
        }

        out
    }
}
