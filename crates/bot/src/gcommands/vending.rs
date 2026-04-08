use crate::gcommands::{GCommand, GContext};
use crate::utils::map::get_grid_pos;
use crate::utils::vending::{get_item_name, resolve_item_id};
use std::future::Future;
use std::pin::Pin;

pub struct Vending;

impl GCommand for Vending {
    fn name(&self) -> &'static str {
        "v"
    }

    fn execute<'a>(
        &'a self,
        ctx: GContext<'a>,
        args: &'a [&'a str],
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<String>>> + Send + 'a>> {
        Box::pin(async move {
            if args.is_empty() {
                return Ok(Some("Usage: !v <item name>".to_string()));
            }

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
        })
    }
}
