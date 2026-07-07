use anyhow::Result;
use poise::serenity_prelude as serenity;
use rustplus::RustPlusClient;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MinibotData {
    pub db_pool: db::DbPool,
    pub rustplus_clients: Arc<Mutex<HashMap<i32, RustPlusClient>>>,
    pub reply_channels: Arc<Mutex<HashMap<i32, serenity::ChannelId>>>,
    pub discord_http: Arc<serenity::Http>,
    pub connection_manager: Arc<Mutex<Option<Arc<crate::connection_manager::ConnectionManager>>>>,
    pub team_queue: Option<Arc<crate::team::TeamQueue>>,
}

#[derive(Clone)]
pub enum ReplyTarget {
    InGameChat { server_id: i32 },
    Discord { channel_id: serenity::ChannelId },
}

pub struct UnifiedContext<'a> {
    pub server_id: i32,
    pub data: &'a MinibotData,
    pub reply_target: ReplyTarget,
    pub discord_id: Option<String>,
    pub steam_id: Option<String>,
}

impl<'a> UnifiedContext<'a> {
    pub async fn reply(&self, message: &str) -> Result<()> {
        match &self.reply_target {
            ReplyTarget::InGameChat { server_id } => {
                let mut clients = self.data.rustplus_clients.lock().await;
                if let Some(client) = clients.get_mut(server_id) {
                    for (i, line) in message.lines().take(4).enumerate() {
                        let mut text = line.trim().to_string();
                        if text.is_empty() {
                            continue;
                        }
                        if text.chars().count() > 100 {
                            text = text.chars().take(97).collect();
                            text.push_str("...");
                        }
                        tracing::info!(
                            "Sending team msg line {} (len {}): {}",
                            i,
                            text.chars().count(),
                            text
                        );
                        if let Err(e) = client.send_team_message(&text).await {
                            tracing::error!("Failed to send team message line {}: {}", i, e);
                            // Facepunch sometimes returns message_not_sent even if it succeeds!
                            // Do not break the loop.
                        }
                        // Increase delay to 1 second to absolutely guarantee we don't trip the spam filter
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    }
                } else {
                    tracing::warn!("No rustplus client connected for server {}", server_id);
                }
            }
            ReplyTarget::Discord { channel_id } => {
                channel_id.say(&self.data.discord_http, message).await?;
            }
        }
        Ok(())
    }

    pub async fn reply_embed(&self, message: &str, thumbnail_url: Option<&str>) -> Result<()> {
        match &self.reply_target {
            ReplyTarget::InGameChat { .. } => {
                self.reply(message).await?;
            }
            ReplyTarget::Discord { channel_id } => {
                let mut embed = serenity::CreateEmbed::new()
                    .description(message)
                    .color(0xCE422B);
                if let Some(url) = thumbnail_url {
                    embed = embed.thumbnail(url);
                }
                let builder = serenity::CreateMessage::new().embed(embed);
                channel_id
                    .send_message(&self.data.discord_http, builder)
                    .await?;
            }
        }
        Ok(())
    }
    pub async fn resolve_emoji(&self, shortname: &str) -> String {
        match &self.reply_target {
            ReplyTarget::InGameChat { .. } => format!(":{}: ", shortname),
            ReplyTarget::Discord { channel_id } => {
                let clean_name = shortname.replace(".", "_").replace("-", "_");

                // Get the guild ID from the channel
                let channel = match channel_id.to_channel(&self.data.discord_http).await {
                    Ok(c) => c,
                    Err(_) => return format!("[{}] ", shortname),
                };

                let guild_id = match channel.guild() {
                    Some(g) => g.guild_id,
                    None => return format!("[{}] ", shortname), // Not in a guild
                };

                // Fetch emojis
                let emojis = match guild_id.emojis(&self.data.discord_http).await {
                    Ok(e) => e,
                    Err(_) => return format!("[{}] ", shortname),
                };

                if let Some(emoji) = emojis.iter().find(|e| e.name == clean_name) {
                    return format!("<:{}:{}> ", emoji.name, emoji.id);
                }

                // Need to upload
                let url = format!("https://cdn.carbonmod.gg/items/{}.png", shortname);
                let img_bytes = match reqwest::get(&url).await {
                    Ok(r) => match r.bytes().await {
                        Ok(b) => b,
                        Err(_) => return format!("[{}] ", shortname),
                    },
                    Err(_) => return format!("[{}] ", shortname),
                };

                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);
                let data_uri = format!("data:image/png;base64,{}", b64);

                match guild_id
                    .create_emoji(&self.data.discord_http, &clean_name, &data_uri)
                    .await
                {
                    Ok(new_emoji) => format!("<:{}:{}> ", new_emoji.name, new_emoji.id),
                    Err(e) => {
                        tracing::warn!("Failed to create emoji {}: {}", clean_name, e);
                        format!("[{}] ", shortname)
                    }
                }
            }
        }
    }
}

use std::pin::Pin;

pub struct CommandResponse {
    pub pages: Vec<String>,
    pub thumbnail_url: Option<String>,
}

impl CommandResponse {
    pub fn text(pages: Vec<String>) -> Self {
        Self {
            pages,
            thumbnail_url: None,
        }
    }
}

pub trait UnifiedCommand: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn execute<'a>(
        &'a self,
        ctx: &'a UnifiedContext<'a>,
        args: &'a [&'a str],
    ) -> Pin<Box<dyn std::future::Future<Output = Result<CommandResponse>> + Send + 'a>>;
}

pub struct CommandRegistry {
    commands: HashMap<String, Arc<dyn UnifiedCommand>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn register<T: UnifiedCommand + 'static>(&mut self, cmd: T) {
        self.commands.insert(cmd.name().to_string(), Arc::new(cmd));
    }

    pub async fn dispatch(
        &self,
        name: &str,
        ctx: &UnifiedContext<'_>,
        args: &[&str],
    ) -> Result<()> {
        if let Some(cmd) = self.commands.get(name) {
            match cmd.execute(ctx, args).await {
                Ok(response) => {
                    let pages = response.pages;
                    if !pages.is_empty() {
                        match &ctx.reply_target {
                            ReplyTarget::InGameChat { .. } => {
                                for page in pages.iter().take(3) {
                                    ctx.reply(page).await?;
                                }
                                if pages.len() > 3 {
                                    ctx.reply(&format!("(+{} more)", pages.len() - 3)).await?;
                                }
                            }
                            ReplyTarget::Discord { .. } => {
                                let thumb = response.thumbnail_url.as_deref();
                                ctx.reply_embed(&pages[0], thumb).await?;
                                if pages.len() > 1 {
                                    ctx.reply(&format!(
                                        "(and {} more pages... use Discord for full view)",
                                        pages.len() - 1
                                    ))
                                    .await?;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    ctx.reply(&format!("Error: {}", e)).await?;
                }
            }
        } else {
            ctx.reply(&format!("Unknown command: {}", name)).await?;
        }
        Ok(())
    }
}
