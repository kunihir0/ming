use db::models::NewVendingTransaction;
use diesel::prelude::*;
use rustplus::RustPlusClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

pub async fn start_vending_monitor(
    db_pool: db::DbPool,
    rustplus_clients: Arc<Mutex<HashMap<i32, RustPlusClient>>>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            tracing::info!("Polling vending machines for market data...");
            
            // Collect map markers from all connected rustplus clients
            let mut clients_lock = rustplus_clients.lock().await;
            for (srv_id, client) in clients_lock.iter_mut() {
                if let Ok(resp) = client.get_map_markers().await {
                    if let Some(map_markers) = resp.response.and_then(|r| r.map_markers) {
                        let markers = map_markers.markers;
                        let mut conn = match db_pool.get() {
                            Ok(c) => c,
                            Err(_) => continue,
                        };
                        let now = chrono::Utc::now().timestamp();
                        
                        for marker in markers {
                                if marker.r#type == 3 { // Vending machine
                                    for item in marker.sell_orders {
                                        let local_item_id = item.item_id;
                                        let local_currency_id = item.currency_id;
                                        
                                        let transaction = NewVendingTransaction {
                                            server_id: *srv_id,
                                            timestamp: now,
                                            item_id: local_item_id,
                                            item_name: crate::items::get_item_name(local_item_id),
                                            currency_id: local_currency_id,
                                            currency_name: crate::items::get_item_name(local_currency_id),
                                            quantity: item.quantity,
                                            cost_per_item: item.cost_per_item,
                                            amount_in_stock: item.amount_in_stock,
                                            is_outlier: 0,
                                        };
                                        use db::schema::vending_transactions::dsl::*;
                                        let _ = diesel::insert_into(vending_transactions)
                                            .values(&transaction)
                                            .execute(&mut conn);
                                    }
                                }
                            }
                    }
                }
            }
        }
    });
}
