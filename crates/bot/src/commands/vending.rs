use crate::Context;
use crate::Error;
use crate::services::vending_subs::VmSub;
use crate::utils::vending::{get_item_name, resolve_item_id};

/// Vending machine commands
#[allow(clippy::unused_async)]
#[poise::command(
    slash_command,
    subcommands("search", "sub", "unsub", "list"),
    subcommand_required
)]
pub async fn v(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Search for an item in vending machines
#[poise::command(slash_command)]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Server ID"] server_id: i32,
    #[description = "Item name to search for"] query: String,
) -> Result<(), Error> {
    let item_id: String = if let Some(id) = resolve_item_id(&query) {
        id
    } else {
        ctx.say(format!("Item '{query}' not found.")).await?;
        return Ok(());
    };

    let item_name = get_item_name(item_id.parse().unwrap_or(0));

    let map_size = ctx
        .data()
        .map_service
        .get_map_size(server_id, ctx.data())
        .await?;
    let vending_machines = ctx
        .data()
        .map_service
        .get_vending_machines(server_id, ctx.data())
        .await?;

    let mut results = vec![];

    for marker in vending_machines {
        for order in &marker.sell_orders {
            if order.item_id.to_string() == item_id && order.amount_in_stock > 0 {
                let grid = crate::utils::map::get_grid_pos(marker.x, marker.y, map_size);
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
        ctx.say(format!("No vending machines found selling {item_name}."))
            .await?;
        return Ok(());
    }

    results.truncate(5);
    let mut response = format!("Found {item_name}:\n");
    response.push_str(&results.join("\n"));
    if results.len() == 5 {
        response.push_str("\n...");
    }

    ctx.say(response).await?;
    Ok(())
}

/// Subscribe to an item in vending machines
#[poise::command(slash_command)]
pub async fn sub(
    ctx: Context<'_>,
    #[description = "Server ID"] server_id: i32,
    #[description = "Item name to subscribe to"] query: String,
    #[description = "Maximum price (optional)"] max_price: Option<i32>,
) -> Result<(), Error> {
    let item_id: String = if let Some(id) = resolve_item_id(&query) {
        id
    } else {
        ctx.say(format!("Item '{query}' not found.")).await?;
        return Ok(());
    };

    let item_id_num = item_id.parse().unwrap_or(0);
    let item_name = get_item_name(item_id_num);

    let sub = VmSub {
        server_id,
        item_id: item_id_num,
        item_name: item_name.clone(),
        max_price,
        max_distance: None, // Can add distance calculation later if needed
        base_location: None,
    };

    ctx.data()
        .sub_store
        .add_sub(ctx.author().id.get(), sub)
        .await;

    ctx.say(format!("Subscribed to {item_name} on server {server_id}."))
        .await?;
    Ok(())
}

/// Unsubscribe from a vending machine alert
#[poise::command(slash_command)]
pub async fn unsub(
    ctx: Context<'_>,
    #[description = "Index of the subscription to remove"] index: usize,
) -> Result<(), Error> {
    // Convert from 1-based index provided by user
    let real_index = if index > 0 { index - 1 } else { 0 };

    if ctx
        .data()
        .sub_store
        .remove_sub(ctx.author().id.get(), real_index)
        .await
    {
        ctx.say("Subscription removed.").await?;
    } else {
        ctx.say("Subscription not found at that index.").await?;
    }

    Ok(())
}

/// List your vending machine subscriptions
#[poise::command(slash_command)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let subs = ctx.data().sub_store.get_subs(ctx.author().id.get()).await;

    if subs.is_empty() {
        ctx.say("You have no active subscriptions.").await?;
        return Ok(());
    }

    let mut response = String::from("**Your Subscriptions:**\n");
    for (i, sub) in subs.iter().enumerate() {
        let price_info = if let Some(mp) = sub.max_price {
            format!(" (Max Price: {mp})")
        } else {
            String::new()
        };

        let _ = std::fmt::Write::write_fmt(
            &mut response,
            format_args!(
                "{}. Server {}: {}{}\n",
                i + 1,
                sub.server_id,
                sub.item_name,
                price_info
            ),
        );
    }

    ctx.say(response).await?;
    Ok(())
}
