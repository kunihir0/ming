use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

use team_dec::TeamDetectorBuilder;

use crate::framework::MinibotData;

type Error = Box<dyn std::error::Error + Send + Sync>;
type PoiseContext<'a> = poise::Context<'a, Arc<MinibotData>, Error>;

pub struct TeamDetectionJob {
    pub server_id: String,
    pub steam_ids: Vec<String>,
    pub requester_id: serenity::UserId,
    pub channel_id: serenity::ChannelId,
}

pub struct TeamQueue {
    sender: mpsc::Sender<TeamDetectionJob>,
}

impl TeamQueue {
    pub fn new(http: Arc<serenity::Http>) -> Self {
        let (sender, mut receiver) = mpsc::channel::<TeamDetectionJob>(100);

        tokio::spawn(async move {
            info!("Team Detection Queue Worker started.");
            while let Some(job) = receiver.recv().await {
                info!(
                    "Processing team detection for {} Steam IDs on Server {}",
                    job.steam_ids.len(), job.server_id
                );

                let start_msg = format!(
                    "Your team detection job for `{}` starting Steam IDs has started. This may take a few minutes...",
                    job.steam_ids.len()
                );
                let mut status_msg = match job.requester_id.create_dm_channel(&http).await {
                    Ok(dm) => dm
                        .send_message(
                            &http,
                            serenity::CreateMessage::new().content(start_msg.clone()),
                        )
                        .await
                        .ok(),
                    Err(_) => None,
                };

                let mut in_channel = false;
                if status_msg.is_none() {
                    status_msg = job
                        .channel_id
                        .send_message(
                            &http,
                            serenity::CreateMessage::new()
                                .content(format!("<@{}> {}", job.requester_id, start_msg)),
                        )
                        .await
                        .ok();
                    in_channel = true;
                }

                let (progress_tx, mut progress_rx) = mpsc::channel::<team_dec::detector::ProgressEvent>(100);
                
                let http_clone = http.clone();
                let mut msg_to_edit = status_msg.clone();
                let updater_handle = tokio::spawn(async move {
                    let mut last_edit = tokio::time::Instant::now();
                    while let Some(evt) = progress_rx.recv().await {
                        // Rate limit edits to once per 2 seconds to avoid Discord rate limits
                        if last_edit.elapsed().as_secs() >= 2 {
                            if let Some(msg) = &mut msg_to_edit {
                                let content = format!(
                                    "```asciidoc\n\
                                    = Team Detection In Progress =\n\
                                    [⌚] Elapsed Time      : {}s\n\
                                    [▼] Search Depth      : {} / {}\n\
                                    [■] Profiles Searched : {} / {}\n\
                                    [≡] Queue Length      : {}\n\n\
                                    = Current Operation =\n\
                                    [►] Scanning : {}\n\
                                    [»] Service  : {}\n\
                                    [~] Action   : {}\n\
                                    [˅] Next Up  : {}\n\n\
                                    = Network Scores =\n\
                                    [+] Confident (Online)  : {}\n\
                                    [?] Potential (Found)   : {}\n\
                                    [⟜] Total Edges Mapped  : {}\n\
                                    [!] Hidden Connections  : {}\n\
                                    ```",
                                    evt.elapsed_secs,
                                    evt.current_depth, evt.max_depth,
                                    evt.profiles_searched, evt.max_profiles,
                                    evt.queue_length,
                                    evt.current_profile,
                                    evt.current_service,
                                    evt.current_action,
                                    evt.next_in_queue.as_deref().unwrap_or("None"),
                                    evt.confident_connections,
                                    evt.potential_connections,
                                    evt.total_edges,
                                    evt.hidden_profiles_found
                                );
                                let _ = msg.edit(&http_clone, serenity::EditMessage::new().content(content)).await;
                            }
                            last_edit = tokio::time::Instant::now();
                        }
                    }
                });

                let detector = TeamDetectorBuilder::new()
                    .debug(false)
                    .recursive_depth(2) // Kept shallow for discord queue to avoid hitting discord timeouts/ratelimits
                    .include_offline(true)
                    .max_profiles(100)
                    .build();

                let scan_start = tokio::time::Instant::now();
                let run_result = detector
                    .run(&job.server_id, job.steam_ids.clone(), Some(progress_tx))
                    .await;
                let elapsed = scan_start.elapsed().as_secs();
                
                // Wait for the updater to naturally finish when progress_tx is dropped inside `run`
                let _ = updater_handle.await;

                // Delete the status message if we can, to clean up before the final embed
                if let Some(msg) = status_msg {
                    let _ = msg.delete(&http).await;
                }

                match run_result {
                    Ok((players, graph)) => {
                        let mut online_on_server = Vec::new();
                        let mut others = Vec::new();

                        for p in &players {
                            if p.is_on_server == Some(true) {
                                online_on_server.push(p);
                            } else {
                                others.push(p);
                            }
                        }
                        
                        let get_path_string = |target_node_id: &str| -> String {
                            use std::collections::{VecDeque, HashMap, HashSet};
                            
                            let mut adj = HashMap::new();
                            for edge in &graph.edges {
                                adj.entry(edge.from.as_str()).or_insert_with(Vec::new).push(edge.to.as_str());
                            }
                            
                            let mut queue = VecDeque::new();
                            let mut visited = HashSet::new();
                            let mut parent = HashMap::new();
                            
                            let seed_nodes: Vec<String> = job.steam_ids.iter().map(|s| format!("s:{}", s)).collect();
                            for seed_node in &seed_nodes {
                                if seed_node == target_node_id {
                                    return String::new(); // It is a seed itself
                                }
                                queue.push_back(seed_node.as_str());
                                visited.insert(seed_node.as_str());
                            }
                            
                            while let Some(current) = queue.pop_front() {
                                if current == target_node_id {
                                    break;
                                }
                                if let Some(neighbors) = adj.get(current) {
                                    for &next in neighbors {
                                        if visited.insert(next) {
                                            parent.insert(next, current);
                                            queue.push_back(next);
                                        }
                                    }
                                }
                            }
                            
                            if !parent.contains_key(target_node_id) {
                                return String::new();
                            }
                            
                            let mut path = Vec::new();
                            let mut curr = target_node_id;
                            while let Some(&p) = parent.get(curr) {
                                path.push(p);
                                curr = p;
                            }
                            path.reverse();
                            
                            let label_map: HashMap<&str, &str> = graph.nodes.iter().map(|n| (n.id.as_str(), n.label.as_str())).collect();
                            let path_names: Vec<&str> = path.into_iter().map(|id| *label_map.get(id).unwrap_or(&id)).collect();
                            
                            if path_names.is_empty() {
                                String::new()
                            } else {
                                format!(" *(via {})*", path_names.join(" ➝ "))
                            }
                        };

                        let chunk_players = |players: &[&team_dec::models::Player], title_prefix: &str, symbol: &str| -> Vec<(String, String)> {
                            if players.is_empty() {
                                return vec![(format!("{} (None)", title_prefix), "*None found.*".to_string())];
                            }
                            
                            let mut chunks = Vec::new();
                            let mut current_chunk = String::new();
                            let mut total_count = 0;
                            let mut start_idx = 1;
                            
                            for p in players {
                                let name_fmt = if symbol == "[+]" { format!("**{}**", p.name) } else { p.name.clone() };
                                
                                let target_node_id = if let Some(s) = &p.steam_id {
                                    format!("s:{}", s)
                                } else if let Some(c) = &p.custom_id {
                                    format!("c:{}", c)
                                } else {
                                    format!("n:{}", p.name)
                                };
                                let path_str = get_path_string(&target_node_id);

                                let line = format!(
                                    "{} [{}](https://steamcommunity.com/profiles/{}){}\n",
                                    symbol,
                                    name_fmt,
                                    p.steam_id.as_deref().unwrap_or("Unknown"),
                                    path_str
                                );
                                
                                if current_chunk.len() + line.len() > 950 {
                                    chunks.push((
                                        format!("{} ({}-{})", title_prefix, start_idx, total_count),
                                        current_chunk.clone()
                                    ));
                                    current_chunk.clear();
                                    start_idx = total_count + 1;
                                }
                                
                                current_chunk.push_str(&line);
                                total_count += 1;
                                
                                // Limit to 10 fields per category to stay well under Discord's 25 field limit
                                if chunks.len() >= 10 {
                                    break;
                                }
                            }
                            
                            if !current_chunk.is_empty() && chunks.len() < 10 {
                                chunks.push((
                                    format!("{} ({}-{})", title_prefix, start_idx, total_count),
                                    current_chunk
                                ));
                            } else if total_count < players.len() {
                                if let Some(last) = chunks.last_mut() {
                                    last.1.push_str(&format!("\n*... and {} more*", players.len() - total_count));
                                }
                            }
                            
                            chunks
                        };

                        let hidden_count = players.iter().filter(|p| p.source_type.as_deref() == Some("hidden_friends")).count();

                        let title_suffix = if job.steam_ids.len() == 1 {
                            job.steam_ids[0].clone()
                        } else {
                            format!("{} starting IDs", job.steam_ids.len())
                        };

                        let mut embed = serenity::CreateEmbed::new()
                            .title(format!("Team Detection Results: {}", title_suffix))
                            .color(0x00FF00)
                            .footer(serenity::CreateEmbedFooter::new(format!(
                                "Graph: {} Nodes, {} Edges | Time: {}s | Hidden Connections: {}",
                                players.len(),
                                graph.edges.len(),
                                elapsed,
                                hidden_count
                            )));

                        let online_fields = chunk_players(&online_on_server, "Confident", "[+]");
                        for (title, content) in online_fields {
                            embed = embed.field(title, content, false);
                        }

                        let offline_fields = chunk_players(&others, "Potential", "[?]");
                        for (title, content) in offline_fields {
                            embed = embed.field(title, content, false);
                        }

                        let dm_result = match job.requester_id.create_dm_channel(&http).await {
                            Ok(dm) => {
                                let msg = serenity::CreateMessage::new().embed(embed.clone());
                                dm.send_message(&http, msg).await
                            }
                            Err(e) => Err(e),
                        };

                        if dm_result.is_err() {
                            let fallback_msg = format!(
                                "<@{}> I couldn't DM you, so here are your results:\n",
                                job.requester_id
                            );
                            let msg = serenity::CreateMessage::new()
                                .content(fallback_msg)
                                .embed(embed);
                            let _ = job.channel_id.send_message(&http, msg).await;
                        } else if in_channel {
                            // Only ping in channel if we couldn't DM the start message, but DMing the result worked
                            // This shouldn't happen usually, but just in case
                            let _ = job.channel_id.send_message(&http, serenity::CreateMessage::new().content(
                                format!("<@{}> Your team detection is complete! I have sent the results to your DMs.", job.requester_id)
                            )).await;
                        }
                    }
                    Err(e) => {
                        error!("Team detection failed: {:?}", e);
                        // Fall back to channel
                        let _ = job
                            .channel_id
                            .send_message(
                                &http,
                                serenity::CreateMessage::new().content(format!(
                                    "<@{}> Your team detection job for {} starting IDs failed: {}",
                                    job.requester_id, job.steam_ids.len(), e
                                )),
                            )
                            .await;
                    }
                }
            }
        });

        Self { sender }
    }

    pub async fn enqueue(
        &self,
        job: TeamDetectionJob,
    ) -> Result<(), mpsc::error::SendError<TeamDetectionJob>> {
        self.sender.send(job).await
    }
}

/// Team Detection Commands
#[poise::command(
    slash_command,
    subcommands("detect"),
    subcommand_required,
    category = "Player Tracking"
)]
pub async fn team(_ctx: PoiseContext<'_>) -> Result<(), Error> {
    Ok(())
}

/// Queue a team detection job for a player
#[poise::command(slash_command)]
async fn detect(
    ctx: PoiseContext<'_>,
    #[description = "BattleMetrics Server ID"] server_id: String,
    #[description = "Primary Steam ID 64"] steam_id1: String,
    #[description = "Additional Steam ID 2"] steam_id2: Option<String>,
    #[description = "Additional Steam ID 3"] steam_id3: Option<String>,
    #[description = "Additional Steam ID 4"] steam_id4: Option<String>,
    #[description = "Additional Steam ID 5"] steam_id5: Option<String>,
    #[description = "Additional Steam ID 6"] steam_id6: Option<String>,
    #[description = "Additional Steam ID 7"] steam_id7: Option<String>,
    #[description = "Additional Steam ID 8"] steam_id8: Option<String>,
    #[description = "Additional Steam ID 9"] steam_id9: Option<String>,
    #[description = "Additional Steam ID 10"] steam_id10: Option<String>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let mut parsed_ids = vec![steam_id1];
    if let Some(id) = steam_id2 { parsed_ids.push(id); }
    if let Some(id) = steam_id3 { parsed_ids.push(id); }
    if let Some(id) = steam_id4 { parsed_ids.push(id); }
    if let Some(id) = steam_id5 { parsed_ids.push(id); }
    if let Some(id) = steam_id6 { parsed_ids.push(id); }
    if let Some(id) = steam_id7 { parsed_ids.push(id); }
    if let Some(id) = steam_id8 { parsed_ids.push(id); }
    if let Some(id) = steam_id9 { parsed_ids.push(id); }
    if let Some(id) = steam_id10 { parsed_ids.push(id); }

    let job = TeamDetectionJob {
        server_id: server_id.clone(),
        steam_ids: parsed_ids.clone(),
        requester_id: ctx.author().id,
        channel_id: ctx.channel_id(),
    };

    if let Some(queue) = &ctx.data().team_queue {
        if queue.enqueue(job).await.is_ok() {
            ctx.say(format!("✅ Job added to the team detection queue for {} Steam IDs on server `{}`. You will be pinged when it starts and completes.", parsed_ids.len(), server_id)).await?;
        } else {
            ctx.say("❌ Failed to add job to the queue. The worker might be offline.")
                .await?;
        }
    } else {
        ctx.say("❌ Team detection queue is not initialized.")
            .await?;
    }

    Ok(())
}
