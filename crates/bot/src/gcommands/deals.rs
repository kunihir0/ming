use crate::gcommands::{GCommand, GContext};
use std::future::Future;
use std::pin::Pin;

use crate::services::ai::find_best_deals;
use poise::serenity_prelude as serenity;

pub struct Deals;

impl GCommand for Deals {
    fn name(&self) -> &'static str {
        "deals"
    }

    fn execute<'a>(
        &'a self,
        ctx: GContext<'a>,
        args: &'a [&'a str],
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<String>>> + Send + 'a>> {
        Box::pin(async move {
            let server_id = ctx.server_id;
            let data = ctx.data.clone();
            
            let query = if args.is_empty() {
                None
            } else {
                Some(args.join(" "))
            };
            
            // We spawn the task so we don't block the team chat loop
            tokio::spawn(async move {
                let map_size = match data.map_service.get_map_size(server_id, &data).await {
                    Ok(size) => size,
                    Err(e) => {
                        tracing::error!("Failed to get map size for AI deals: {e}");
                        return;
                    }
                };

                let vending_machines = match data.map_service.get_vending_machines(server_id, &data).await {
                    Ok(vms) => vms,
                    Err(e) => {
                        tracing::error!("Failed to get vending machines for AI deals: {e}");
                        return;
                    }
                };

                let best_deals = match find_best_deals(server_id, map_size, &vending_machines, query.clone()).await {
                    Ok(deals) => deals,
                    Err(e) => format!("AI Analysis Error: {e}"),
                };

                use db::models::ServerChannel;
                use db::schema::server_channels::dsl as sc_dsl;
                use diesel::prelude::*;

                let chat_channel_id_str = {
                    let mut conn = match data.db_pool.get() {
                        Ok(conn) => conn,
                        Err(e) => {
                            tracing::error!("Failed to get db conn for AI deals: {e}");
                            return;
                        }
                    };

                    let server_channel = sc_dsl::server_channels
                        .filter(sc_dsl::server_id.eq(server_id))
                        .first::<ServerChannel>(&mut conn)
                        .optional()
                        .unwrap_or(None);

                    server_channel.and_then(|sc| sc.chat_channel_id)
                };

                if let Some(channel_id_str) = chat_channel_id_str {
                    if let Ok(channel_id_u64) = channel_id_str.parse::<u64>() {
                        let channel_id = serenity::ChannelId::new(channel_id_u64);
                        
                        let message = format!("**AI Vending Machine Analysis**\n\n{best_deals}");
                        
                        if message.len() > 2000 {
                            let mut current = String::new();
                            for line in message.lines() {
                                if current.len() + line.len() > 1900 {
                                    let _ = channel_id.say(&data.discord_http, &current).await;
                                    current.clear();
                                }
                                current.push_str(line);
                                current.push('\n');
                            }
                            if !current.is_empty() {
                                let _ = channel_id.say(&data.discord_http, &current).await;
                            }
                        } else {
                            if let Err(e) = channel_id.say(&data.discord_http, &message).await {
                                tracing::error!("Failed to send AI deals to Discord: {e}");
                            }
                        }
                    }
                } else {
                    tracing::warn!("No chat channel configured for server {server_id} to post AI deals");
                }
            });

            Ok(Some("Analyzing vending machines... Results will be posted to Discord.".to_string()))
        })
    }
}
