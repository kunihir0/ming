use crate::framework::{UnifiedCommand, UnifiedContext};
use crate::items::search_items_smart;
use anyhow::Result;
use db::models::{NewVendingSubscription, VendingSubscription};
use db::schema::vending_subscriptions::dsl::*;
use diesel::prelude::*;
use std::future::Future;
use std::pin::Pin;

pub struct VendingSearchCommand;

impl UnifiedCommand for VendingSearchCommand {
    fn name(&self) -> &'static str {
        "search"
    }

    fn description(&self) -> &'static str {
        "Smart search for items in vending machines (supports regex/wildcards)"
    }

    fn execute<'a>(&'a self, ctx: &'a UnifiedContext<'a>, args: &'a [&'a str]) -> Pin<Box<dyn Future<Output = Result<crate::framework::CommandResponse>> + Send + 'a>> {
        Box::pin(async move {
            if args.is_empty() {
                return Ok(crate::framework::CommandResponse::text(vec!["Usage: v search [buy|sell] <item name or regex>".to_string()]));
            }

            let mut search_type = "buy";
            let mut query_start = 0;

            if args[0].eq_ignore_ascii_case("buy") {
                search_type = "buy";
                query_start = 1;
            } else if args[0].eq_ignore_ascii_case("sell") {
                search_type = "sell";
                query_start = 1;
            }

            if query_start >= args.len() {
                return Ok(crate::framework::CommandResponse::text(vec!["Usage: v search [buy|sell] <item name or regex>".to_string()]));
            }

            let is_discord = matches!(ctx.reply_target, crate::framework::ReplyTarget::Discord { .. });

            let query_joined = args[query_start..].join(" ");
            let queries: Vec<&str> = if is_discord {
                query_joined
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                let trimmed = query_joined.trim();
                if trimmed.is_empty() {
                    vec![]
                } else {
                    vec![trimmed]
                }
            };
                
            let mut target_ids = std::collections::HashSet::new();
            for q in &queries {
                let matches = search_items_smart(q);
                if !matches.is_empty() {
                    target_ids.insert(matches[0].0);
                }
            }

            if target_ids.is_empty() {
                return Ok(crate::framework::CommandResponse::text(vec![format!("No items found matching '{}'.", queries.join(", "))]));
            }

            let mut clients = ctx.data.rustplus_clients.lock().await;
            let client = match clients.get_mut(&ctx.server_id) {
                Some(c) => c,
                None => {
                    return Ok(crate::framework::CommandResponse::text(vec!["Rust+ client not connected for this server.".to_string()]));
                }
            };

            let mut markers_res = None;
            for _ in 0..3 {
                match client.get_map_markers().await {
                    Ok(resp) => {
                        markers_res = Some(resp.response.unwrap().map_markers.unwrap().markers);
                        break;
                    }
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }

            let markers = match markers_res {
                Some(m) => m,
                None => return Ok(crate::framework::CommandResponse::text(vec!["Failed to fetch map markers".to_string()])),
            };

            let mut map_size_res = None;
            for _ in 0..3 {
                match client.get_info().await {
                    Ok(info) => {
                        map_size_res = Some(info.response.unwrap().info.unwrap().map_size);
                        break;
                    }
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }

            let map_size = match map_size_res {
                Some(s) => s as f32,
                None => return Ok(crate::framework::CommandResponse::text(vec!["Failed to fetch map info".to_string()])),
            };

            struct MatchEntry {
                group_name: String,
                group_short: Option<String>,
                target_name: String,
                cost: i32,
                quantity: i32,
                pos: String,
                stock: i32,
            }

            let mut matches_list = Vec::new();
            for marker in markers {
                for order in marker.sell_orders {
                    let is_match = if search_type == "sell" {
                        target_ids.contains(&order.currency_id)
                    } else {
                        target_ids.contains(&order.item_id)
                    };

                    if is_match && order.amount_in_stock > 0 {
                        let (group_name, group_short) = if search_type == "sell" {
                            (crate::items::get_item_name(order.item_id), crate::items::get_item_shortname(order.item_id))
                        } else {
                            (crate::items::get_item_name(order.currency_id), crate::items::get_item_shortname(order.currency_id))
                        };
                        
                        let target_name = if search_type == "sell" {
                            crate::items::get_item_name(order.currency_id)
                        } else {
                            crate::items::get_item_name(order.item_id)
                        };

                        let pos = get_grid_pos(marker.x, marker.y, map_size as u32);
                        
                        matches_list.push(MatchEntry {
                            group_name,
                            group_short,
                            target_name,
                            cost: order.cost_per_item,
                            quantity: order.quantity,
                            pos,
                            stock: order.amount_in_stock,
                        });
                    }
                }
            }

            if matches_list.is_empty() {
                return Ok(crate::framework::CommandResponse::text(vec!["No machines found.".to_string()]));
            } else {
                // Sort by cost ascending
                matches_list.sort_by_key(|m| m.cost);

                let mut groups: std::collections::BTreeMap<(String, Option<String>), Vec<(i32, i32, String, i32, String)>> = std::collections::BTreeMap::new();
                for m in matches_list {
                    groups.entry((m.group_name, m.group_short)).or_default().push((m.cost, m.quantity, m.pos, m.stock, m.target_name));
                }

                let max_len = if is_discord { 3500 } else { 100 };
                
                let mut pages = Vec::new();
                for ((group_name, group_short), entries) in groups {
                    let group_display = if let Some(short) = group_short {
                        let emoji = ctx.resolve_emoji(&short).await;
                        if is_discord {
                            if search_type == "buy" {
                                format!("{} **Costs {}**\n", emoji.trim(), group_name)
                            } else {
                                format!("{} **Pays {}**\n", emoji.trim(), group_name)
                            }
                        } else {
                            emoji
                        }
                    } else {
                        if is_discord {
                            if search_type == "buy" {
                                format!("**Costs {}**\n", group_name)
                            } else {
                                format!("**Pays {}**\n", group_name)
                            }
                        } else {
                            format!("[{}] ", group_name)
                        }
                    };
                    
                    let mut current_page = group_display.clone();
                    let prefix_len = current_page.len();

                    for (cost, qty, pos, stock, target_name) in entries {
                        let formatted_entry = if is_discord {
                            if search_type == "buy" {
                                format!("> `{}x {}` for `{}x {}` | **{}** ({} left)\n", qty, target_name, cost, group_name, pos, stock)
                            } else {
                                format!("> `{}x {}` for `{}x {}` | **{}** ({} left)\n", cost, target_name, qty, group_name, pos, stock)
                            }
                        } else {
                            if search_type == "buy" {
                                format!("{}x {} | {}({})", qty, cost, pos, stock)
                            } else {
                                format!("{}x {} | {}({})", cost, qty, pos, stock)
                            }
                        };

                        let separator = if is_discord { "" } else { ", " };
                        let needs_separator = current_page.len() > prefix_len;
                        let extra = if needs_separator { separator.len() } else { 0 };

                        if current_page.len() + formatted_entry.len() + extra > max_len {
                            pages.push(current_page.clone());
                            current_page = format!("{}{}", group_display, formatted_entry);
                        } else {
                            if needs_separator {
                                current_page.push_str(separator);
                            }
                            current_page.push_str(&formatted_entry);
                        }
                    }
                    if is_discord {
                        current_page.push('\n');
                    }
                    if current_page.len() > prefix_len + if is_discord { 1 } else { 0 } {
                        pages.push(current_page);
                    }
                }
                
                // For Discord, we can just join pages if they fit in 3500.
                if is_discord && pages.len() > 1 {
                    let mut combined_pages = Vec::new();
                    let mut current = String::new();
                    for page in pages {
                        if current.len() + page.len() > 3500 {
                            combined_pages.push(current);
                            current = page;
                        } else {
                            current.push_str(&page);
                        }
                    }
                    if !current.is_empty() {
                        combined_pages.push(current);
                    }
                    pages = combined_pages;
                }
                
                Ok(crate::framework::CommandResponse {
                    pages,
                    thumbnail_url: None,
                })
            }
        })
    }
}

pub struct VendingSubsCommand;

impl UnifiedCommand for VendingSubsCommand {
    fn name(&self) -> &'static str {
        "subs"
    }

    fn description(&self) -> &'static str {
        "Subscribe to vending machine updates for an item"
    }

    fn execute<'a>(&'a self, ctx: &'a UnifiedContext<'a>, args: &'a [&'a str]) -> Pin<Box<dyn Future<Output = Result<crate::framework::CommandResponse>> + Send + 'a>> {
        Box::pin(async move {
            if args.is_empty() {
                return Ok(crate::framework::CommandResponse::text(vec!["Usage: v subs <add|remove|list> [item]".to_string()]));
            }

            let query = args.join(" ");
            let matches = crate::items::search_items_smart(&query);

            if matches.is_empty() {
                return Ok(crate::framework::CommandResponse::text(vec![format!("No items found matching '{}'.", query)]));
            }

            let (item_id_val, target_name, _shortname) = &matches[0];
            
            let mut conn = ctx.data.db_pool.get()?;
            
            let new_sub = NewVendingSubscription {
                discord_id: ctx.discord_id.clone(),
                steam_id: ctx.steam_id.clone(),
                server_id: ctx.server_id,
                item_id: *item_id_val,
                item_name: target_name.clone(),
                max_price: None,
            };

            diesel::insert_into(vending_subscriptions)
                .values(&new_sub)
                .execute(&mut conn)?;

            Ok(crate::framework::CommandResponse::text(vec![format!("Done! Subscriptions for {} modified.", target_name)]))
        })
    }
}

pub struct VendingListCommand;

impl UnifiedCommand for VendingListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    fn description(&self) -> &'static str {
        "List all your active vending machine subscriptions"
    }

    fn execute<'a>(&'a self, ctx: &'a UnifiedContext<'a>, _args: &'a [&'a str]) -> Pin<Box<dyn Future<Output = Result<crate::framework::CommandResponse>> + Send + 'a>> {
        Box::pin(async move {
            let mut conn = ctx.data.db_pool.get()?;
            
            use db::schema::vending_subscriptions::dsl::*;
            use diesel::prelude::*;
            
            let mut q = vending_subscriptions
                .filter(server_id.eq(ctx.server_id))
                .into_boxed();
                
            if let Some(did) = &ctx.discord_id {
                q = q.filter(discord_id.eq(did));
            } else if let Some(sid) = &ctx.steam_id {
                q = q.filter(steam_id.eq(sid));
            } else {
                return Ok(crate::framework::CommandResponse::text(vec!["Could not identify user for subscriptions.".to_string()]));
            }

            let subs = q.load::<VendingSubscription>(&mut conn)?;
            
            if subs.is_empty() {
                return Ok(crate::framework::CommandResponse::text(vec!["You have no active subscriptions.".to_string()]));
            }
            
            let mut response = String::from("Your Subscriptions:\n");
            for (i, sub) in subs.iter().enumerate() {
                response.push_str(&format!("{}. {}\n", i + 1, sub.item_name));
            }
            Ok(crate::framework::CommandResponse::text(vec![response]))
        })
    }
}

const GRID_DIAMETER: f32 = 146.25;

fn get_grid_pos(x: f32, y: f32, map_size: u32) -> String {
    #[allow(clippy::cast_precision_loss)]
    let map_size_f = map_size as f32;
    let corrected_map_size = get_corrected_map_size(map_size_f);

    if x < 0.0 || x > corrected_map_size || y < 0.0 || y > corrected_map_size {
        return "Outside Grid".to_string();
    }

    let grid_pos_letters = get_grid_pos_letters_x(x);
    let grid_pos_number = get_grid_pos_number_y(y, corrected_map_size);

    format!("{grid_pos_letters}{grid_pos_number}")
}

fn get_grid_pos_letters_x(x: f32) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let counter = (x / GRID_DIAMETER).floor() as u32 + 1;
    number_to_letters(counter)
}

fn get_grid_pos_number_y(y: f32, map_size: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let number_of_grids = (map_size / GRID_DIAMETER).floor() as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let counter = (y / GRID_DIAMETER).floor() as u32 + 1;
    number_of_grids.saturating_sub(counter)
}

fn number_to_letters(mut num: u32) -> String {
    let mut letters = String::new();
    while num > 0 {
        let mod_val = (num - 1) % 26;
        letters.insert(0, (b'A' + mod_val as u8) as char);
        num = (num - mod_val) / 26;
    }
    letters
}

fn get_corrected_map_size(map_size: f32) -> f32 {
    let remainder = map_size % GRID_DIAMETER;
    if remainder < 120.0 {
        map_size - remainder
    } else {
        map_size + (GRID_DIAMETER - remainder)
    }
}
