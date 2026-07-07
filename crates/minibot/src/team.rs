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
    pub steam_id: String,
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
                    "Processing team detection for Steam ID {} on Server {}",
                    job.steam_id, job.server_id
                );

                let start_msg = format!(
                    "Your team detection job for `{}` has started. This may take a few minutes...",
                    job.steam_id
                );
                // Try DM first to avoid public intel leak
                let dm_start_success = match job.requester_id.create_dm_channel(&http).await {
                    Ok(dm) => dm
                        .send_message(
                            &http,
                            serenity::CreateMessage::new().content(start_msg.clone()),
                        )
                        .await
                        .is_ok(),
                    Err(_) => false,
                };

                if !dm_start_success {
                    let _ = job
                        .channel_id
                        .send_message(
                            &http,
                            serenity::CreateMessage::new()
                                .content(format!("<@{}> {}", job.requester_id, start_msg)),
                        )
                        .await;
                }

                let detector = TeamDetectorBuilder::new()
                    .debug(false)
                    .recursive_depth(2) // Kept shallow for discord queue to avoid hitting discord timeouts/ratelimits
                    .include_offline(true)
                    .max_profiles(100)
                    .build();

                match detector
                    .run(&job.server_id, vec![job.steam_id.clone()])
                    .await
                {
                    Ok((players, _graph)) => {
                        let mut online_on_server = Vec::new();
                        let mut others = Vec::new();

                        for p in players {
                            if p.is_on_server == Some(true) {
                                online_on_server.push(p);
                            } else {
                                others.push(p);
                            }
                        }

                        let mut online_desc = String::new();
                        if online_on_server.is_empty() {
                            online_desc.push_str(
                                "*No linked players found currently ONLINE on this server.*\n",
                            );
                        } else {
                            for p in &online_on_server {
                                online_desc.push_str(&format!(
                                    "🟢 **{}** (`{}`)\n",
                                    p.name,
                                    p.steam_id.as_deref().unwrap_or("Unknown")
                                ));
                            }
                        }

                        let mut offline_desc = String::new();
                        let max_offline = 20;
                        let offline_count = others.len();
                        for p in others.into_iter().take(max_offline) {
                            offline_desc.push_str(&format!(
                                "⚪ {} (`{}`)\n",
                                p.name,
                                p.steam_id.as_deref().unwrap_or("Unknown")
                            ));
                        }
                        if offline_count > max_offline {
                            offline_desc.push_str(&format!(
                                "\n*... and {} more offline/other servers.*",
                                offline_count - max_offline
                            ));
                        }
                        if offline_count == 0 {
                            offline_desc.push_str("*None found.*");
                        }

                        let embed = serenity::CreateEmbed::new()
                            .title(format!("Team Detection Results: {}", job.steam_id))
                            .color(0x00FF00)
                            .field("Online on Server", online_desc, false)
                            .field("Offline / Other Servers", offline_desc, false);

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
                        } else if !dm_start_success {
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
                                    "<@{}> Your team detection job for `{}` failed: {}",
                                    job.requester_id, job.steam_id, e
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
    #[description = "Steam ID 64"] steam_id: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let job = TeamDetectionJob {
        server_id: server_id.clone(),
        steam_id: steam_id.clone(),
        requester_id: ctx.author().id,
        channel_id: ctx.channel_id(),
    };

    if let Some(queue) = &ctx.data().team_queue {
        if queue.enqueue(job).await.is_ok() {
            ctx.say(format!("✅ Job added to the team detection queue for Steam ID `{}` on server `{}`. You will be pinged when it starts and completes.", steam_id, server_id)).await?;
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
