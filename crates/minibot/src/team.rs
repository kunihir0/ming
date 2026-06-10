use std::sync::Arc;
use tokio::sync::mpsc;
use poise::serenity_prelude as serenity;
use tracing::{info, error};

use team_dec::{TeamDetectorBuilder, TeamDetectorConfig};

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
                info!("Processing team detection for Steam ID {} on Server {}", job.steam_id, job.server_id);
                
                // Notify user it started
                let _ = job.channel_id.send_message(&http, serenity::CreateMessage::new().content(
                    format!("<@{}> Your team detection job for `{}` has started. This may take a few minutes...", job.requester_id, job.steam_id)
                )).await;

                let detector = TeamDetectorBuilder::new()
                    .debug(false)
                    .recursive_depth(2) // Kept shallow for discord queue to avoid hitting discord timeouts/ratelimits
                    .include_offline(true)
                    .max_profiles(100)
                    .build();

                match detector.run(&job.server_id, vec![job.steam_id.clone()]).await {
                    Ok((players, _graph)) => {
                        let mut content = format!("**Team Detection Results for `{}`**\n\n", job.steam_id);
                        
                        let mut online_on_server = Vec::new();
                        let mut others = Vec::new();

                        for p in players {
                            if p.is_on_server == Some(true) {
                                online_on_server.push(p);
                            } else {
                                others.push(p);
                            }
                        }

                        if online_on_server.is_empty() {
                            content.push_str("No linked players found currently ONLINE on this server.\n");
                        } else {
                            content.push_str("🟢 **ONLINE ON SERVER:**\n```diff\n");
                            for p in online_on_server {
                                content.push_str(&format!("+ {} ({})\n", p.name, p.steam_id.unwrap_or_default()));
                            }
                            content.push_str("```\n");
                        }

                        if !others.is_empty() {
                            content.push_str("\n⚪ **Offline/Other Servers:**\n```diff\n");
                            for p in others.into_iter().take(20) { // Limit to avoid hitting discord message limits
                                content.push_str(&format!("--- {} ({})\n", p.name, p.steam_id.unwrap_or_default()));
                            }
                            content.push_str("```\n");
                        }

                        let _ = job.requester_id.create_dm_channel(&http).await
                            .unwrap()
                            .send_message(&http, serenity::CreateMessage::new().content(content))
                            .await;
                            
                        let _ = job.channel_id.send_message(&http, serenity::CreateMessage::new().content(
                            format!("<@{}> Your team detection is complete! I have sent the results to your DMs.", job.requester_id)
                        )).await;
                    }
                    Err(e) => {
                        error!("Team detection failed: {:?}", e);
                        let _ = job.channel_id.send_message(&http, serenity::CreateMessage::new().content(
                            format!("<@{}> Your team detection job for `{}` failed: {}", job.requester_id, job.steam_id, e)
                        )).await;
                    }
                }
            }
        });

        Self { sender }
    }

    pub async fn enqueue(&self, job: TeamDetectionJob) -> Result<(), mpsc::error::SendError<TeamDetectionJob>> {
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
    #[description = "BattleMetrics Server ID"]
    server_id: String,
    #[description = "Steam ID 64"]
    steam_id: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let job = TeamDetectionJob {
        server_id: server_id.clone(),
        steam_id: steam_id.clone(),
        requester_id: ctx.author().id,
        channel_id: ctx.channel_id(),
    };

    if let Some(queue) = &ctx.data().team_queue {
        let _ = queue.enqueue(job).await;
        ctx.say(format!("✅ Job added to the team detection queue for Steam ID `{}` on server `{}`. You will be pinged when it starts and completes.", steam_id, server_id)).await?;
    } else {
        ctx.say("❌ Team detection queue is not initialized.").await?;
    }

    Ok(())
}
