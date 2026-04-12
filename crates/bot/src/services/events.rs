use poise::serenity_prelude as serenity;
use rustplus::events::RustEvent;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, broadcast};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum EventKind {
    Cargo,
    Heli,
    OilRig,
    Ch47,
    VendingMachine,
}

pub struct Notifier {
    http: Arc<serenity::Http>,
    config: HashMap<EventKind, serenity::ChannelId>,
    last_sent: Mutex<HashMap<EventKind, Instant>>,
    cooldown: Duration,
}

impl Notifier {
    #[must_use]
    pub fn new(http: Arc<serenity::Http>, config: HashMap<EventKind, serenity::ChannelId>) -> Self {
        Self {
            http,
            config,
            last_sent: Mutex::new(HashMap::new()),
            cooldown: Duration::from_secs(60), // 1 min cooldown per event kind
        }
    }

    #[allow(clippy::collapsible_if)]
    pub async fn send(&self, kind: EventKind, message: String) {
        let Some(&channel_id) = self.config.get(&kind) else {
            return; // No channel configured for this event
        };

        let mut last_sent = self.last_sent.lock().await;
        if let Some(last) = last_sent.get(&kind) {
            if last.elapsed() < self.cooldown {
                warn!("Event {:?} is on cooldown, skipping message.", kind);
                return;
            }
        }

        match channel_id.say(&self.http, &message).await {
            Ok(_) => {
                info!("Sent event notification for {:?}: {}", kind, message);
                last_sent.insert(kind, Instant::now());
            }
            Err(e) => {
                error!("Failed to send event notification for {:?}: {}", kind, e);
            }
        }
    }
}

pub struct EventRouter {
    server_id: i32,
    notifier: Arc<Notifier>,
    sub_store: Arc<crate::services::vending_subs::SubStore>,
    team_chat_tx: Option<tokio::sync::mpsc::Sender<String>>,
}

impl EventRouter {
    #[must_use]
    pub fn new(
        server_id: i32,
        notifier: Arc<Notifier>,
        sub_store: Arc<crate::services::vending_subs::SubStore>,
        team_chat_tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> Self {
        Self {
            server_id,
            notifier,
            sub_store,
            team_chat_tx,
        }
    }

    #[allow(clippy::too_many_lines, clippy::collapsible_if)]
    pub async fn run(self, mut rx: broadcast::Receiver<RustEvent>) {
        while let Ok(event) = rx.recv().await {
            match event {
                RustEvent::CargoSpawned => {
                    self.notifier
                        .send(
                            EventKind::Cargo,
                            "🚢 **Cargo Ship** has entered the map!".to_string(),
                        )
                        .await;
                }
                RustEvent::CargoDespawned { was_out_for } => {
                    let mins = was_out_for.as_secs() / 60;
                    self.notifier
                        .send(
                            EventKind::Cargo,
                            format!("🚢 **Cargo Ship** has left the map (was out for {mins}m)."),
                        )
                        .await;
                }
                RustEvent::CargoEgress { spawned_at } => {
                    let mins = spawned_at.elapsed().as_secs() / 60;
                    self.notifier
                        .send(
                            EventKind::Cargo,
                            format!("🚢 **Cargo Ship** is leaving the map (was out for {mins}m)."),
                        )
                        .await;
                }
                RustEvent::HeliSpawned => {
                    self.notifier
                        .send(
                            EventKind::Heli,
                            "🚁 **Patrol Helicopter** has spawned!".to_string(),
                        )
                        .await;
                }
                RustEvent::HeliDespawned { was_out_for } => {
                    let mins = was_out_for.as_secs() / 60;
                    self.notifier
                        .send(
                            EventKind::Heli,
                            format!(
                                "🚁 **Patrol Helicopter** has despawned (was out for {mins}m)."
                            ),
                        )
                        .await;
                }
                RustEvent::HeliTakenDown { last_position } => {
                    self.notifier
                        .send(
                            EventKind::Heli,
                            format!(
                                "💥 **Patrol Helicopter** was taken down at `{:.0}, {:.0}`!",
                                last_position.0, last_position.1
                            ),
                        )
                        .await;
                }
                RustEvent::OilRigCrateDropped { unlock_at: _ } => {
                    self.notifier
                        .send(
                            EventKind::OilRig,
                            "🛢️ **Oil Rig** crate timer has started!".to_string(),
                        )
                        .await;
                }
                RustEvent::OilRigCrateLooted => {
                    self.notifier
                        .send(
                            EventKind::OilRig,
                            "🛢️ **Oil Rig** crate has been looted!".to_string(),
                        )
                        .await;
                }
                RustEvent::Ch47Entered => {
                    self.notifier
                        .send(
                            EventKind::Ch47,
                            "🚁 **CH47 Chinook** has entered the map!".to_string(),
                        )
                        .await;
                }
                RustEvent::Ch47Left => {
                    self.notifier
                        .send(
                            EventKind::Ch47,
                            "🚁 **CH47 Chinook** has left the map.".to_string(),
                        )
                        .await;
                }
                RustEvent::VendingMachineNew { position, id } => {
                    self.notifier
                        .send(
                            EventKind::VendingMachine,
                            format!(
                                "🏪 **New Vending Machine** ({id}) spawned at `{:.0}, {:.0}`.",
                                position.0, position.1
                            ),
                        )
                        .await;
                }
                RustEvent::MarkerSnapshot(markers) => {
                    let subs_by_user = self.sub_store.get_all().await;

                    for (user_id, subs) in subs_by_user {
                        for sub in subs {
                            if sub.server_id != self.server_id {
                                continue;
                            }

                            for marker in &markers {
                                if marker.r#type() != rustplus::proto::AppMarkerType::VendingMachine
                                {
                                    continue;
                                }

                                for order in &marker.sell_orders {
                                    if order.item_id == sub.item_id && order.amount_in_stock > 0 {
                                        // Max price check
                                        if let Some(max_price) = sub.max_price {
                                            if order.cost_per_item > max_price {
                                                continue;
                                            }
                                        }

                                        // Distance check
                                        if let Some(max_dist) = sub.max_distance {
                                            if let Some((bx, by)) = sub.base_location {
                                                let dx = marker.x - bx;
                                                let dy = marker.y - by;
                                                let dist = (dx * dx + dy * dy).sqrt();
                                                if dist > max_dist {
                                                    continue;
                                                }
                                            }
                                        }

                                        let currency_name =
                                            crate::utils::vending::get_item_name(order.currency_id);

                                        #[allow(clippy::cast_sign_loss)]
                                        let in_game = user_id == (self.server_id as u64);

                                        if in_game {
                                            if let Some(tx) = &self.team_chat_tx {
                                                let msg = format!(
                                                    "[Vending Alert]: {} x{} for {} {} at {:.0}, {:.0} (Stock: {})",
                                                    sub.item_name,
                                                    order.quantity,
                                                    order.cost_per_item,
                                                    currency_name,
                                                    marker.x,
                                                    marker.y,
                                                    order.amount_in_stock
                                                );
                                                let _ = tx.send(msg).await;
                                            }
                                        } else {
                                            // Send direct message for Discord users
                                            if let Ok(user) = serenity::UserId::new(user_id)
                                                .to_user(&self.notifier.http)
                                                .await
                                            {
                                                let msg = format!(
                                                    "🏪 **Vending Alert**: Found {} selling x{} for {} {} at `{:.0}, {:.0}` (Stock: {})",
                                                    sub.item_name,
                                                    order.quantity,
                                                    order.cost_per_item,
                                                    currency_name,
                                                    marker.x,
                                                    marker.y,
                                                    order.amount_in_stock
                                                );
                                                let _ = user
                                                    .direct_message(
                                                        &self.notifier.http,
                                                        serenity::CreateMessage::new().content(msg),
                                                    )
                                                    .await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
