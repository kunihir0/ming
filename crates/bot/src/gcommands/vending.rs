use crate::gcommands::{GCommand, GContext};
use crate::services::vending_subs::VmSub;
use crate::utils::map::get_grid_pos;
use crate::utils::vending::{get_item_name, resolve_item_id};
use std::future::Future;
use std::pin::Pin;

pub struct Vending;

impl GCommand for Vending {
    fn name(&self) -> &'static str {
        "v"
    }

    #[allow(clippy::too_many_lines)]
    fn execute<'a>(
        &'a self,
        ctx: GContext<'a>,
        args: &'a [&'a str],
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<String>>> + Send + 'a>> {
        Box::pin(async move {
            if args.is_empty() {
                return Ok(Some(
                    "Usage: !v <item> | !v sub <item> [max price] | !v unsub <id> | !v list"
                        .to_string(),
                ));
            }

            let subcmd = args[0].to_lowercase();
            #[allow(clippy::cast_sign_loss)]
            let key = ctx.server_id as u64;

            match subcmd.as_str() {
                "sub" => {
                    if args.len() < 2 {
                        return Ok(Some("Usage: !v sub <item name> [max_price]".to_string()));
                    }

                    // Parse max price if the last argument is a number
                    let mut max_price = None;
                    let mut query_args = &args[1..];

                    if let Ok(price) = query_args.last().unwrap().parse::<i32>() {
                        max_price = Some(price);
                        query_args = &query_args[..query_args.len() - 1];
                    }

                    if query_args.is_empty() {
                        return Ok(Some("Usage: !v sub <item name> [max_price]".to_string()));
                    }

                    let query = query_args.join(" ");
                    let item_id: String = match resolve_item_id(&query) {
                        Some(id) => id,
                        None => return Ok(Some(format!("Item '{query}' not found."))),
                    };

                    let item_id_num = item_id.parse().unwrap_or(0);
                    let item_name = get_item_name(item_id_num);

                    let sub = VmSub {
                        server_id: ctx.server_id,
                        item_id: item_id_num,
                        item_name: item_name.clone(),
                        max_price,
                        max_distance: None,
                        base_location: None,
                    };

                    ctx.data.sub_store.add_sub(key, sub).await;

                    let price_text = match max_price {
                        Some(p) => format!(" (max price: {p})"),
                        None => String::new(),
                    };
                    Ok(Some(format!("Subscribed to {item_name}{price_text}")))
                }
                "unsub" => {
                    if args.len() < 2 {
                        return Ok(Some("Usage: !v unsub <index>".to_string()));
                    }
                    let Ok(index) = args[1].parse::<usize>() else {
                        return Ok(Some("Invalid index.".to_string()));
                    };
                    let real_index = if index > 0 { index - 1 } else { 0 };

                    if ctx.data.sub_store.remove_sub(key, real_index).await {
                        Ok(Some(format!("Unsubscribed from alert #{index}")))
                    } else {
                        Ok(Some(format!("Subscription #{index} not found.")))
                    }
                }
                "list" => {
                    let subs = ctx.data.sub_store.get_subs(key).await;
                    if subs.is_empty() {
                        return Ok(Some("No active vending subscriptions.".to_string()));
                    }

                    let mut results = vec!["Subscriptions:".to_string()];
                    for (i, sub) in subs.iter().enumerate() {
                        let price_info = match sub.max_price {
                            Some(mp) => format!(" (max: {mp})"),
                            None => String::new(),
                        };
                        results.push(format!("{}. {}{price_info}", i + 1, sub.item_name));
                    }

                    // Truncate to avoid huge team chat messages
                    if results.len() > 10 {
                        results.truncate(10);
                        results.push("...".to_string());
                    }

                    Ok(Some(results.join("\n")))
                }
                _ => {
                    // Default to search
                    let query = args.join(" ");
                    let item_id: String = match resolve_item_id(&query) {
                        Some(id) => id,
                        None => return Ok(Some(format!("Item '{query}' not found."))),
                    };

                    let item_name = get_item_name(match item_id.parse() {
                        Ok(id) => id,
                        Err(_) => 0,
                    });

                    // Use MapService to fetch size and vending machines
                    let map_size = ctx
                        .data
                        .map_service
                        .get_map_size(ctx.server_id, ctx.data)
                        .await?;
                    let vending_machines = ctx
                        .data
                        .map_service
                        .get_vending_machines(ctx.server_id, ctx.data)
                        .await?;

                    let mut results = vec![];

                    for marker in vending_machines {
                        for order in &marker.sell_orders {
                            if order.item_id.to_string() == item_id && order.amount_in_stock > 0 {
                                let grid = get_grid_pos(marker.x, marker.y, map_size);
                                let currency_name = get_item_name(order.currency_id);
                                results.push(format!(
                                    "[{grid}] x{qty} for {cost} {curr} ({stock} in stock)",
                                    qty = order.quantity,
                                    cost = order.cost_per_item,
                                    curr = currency_name,
                                    stock = order.amount_in_stock
                                ));
                            }
                        }
                    }

                    if results.is_empty() {
                        return Ok(Some(format!(
                            "No vending machines found selling {item_name}."
                        )));
                    }

                    results.truncate(5);
                    let mut response = format!("Found {item_name}:\n");
                    response.push_str(&results.join("\n"));
                    if results.len() == 5 {
                        response.push_str("\n...");
                    }

                    Ok(Some(response))
                }
            }
        })
    }
}
