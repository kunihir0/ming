use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSub {
    pub server_id: i32,
    pub item_id: i32,
    pub item_name: String,
    pub max_price: Option<i32>,
    pub max_distance: Option<f32>,
    pub base_location: Option<(f32, f32)>,
}

#[derive(Clone)]
pub struct SubStore {
    // UserId -> Subscriptions
    subs: Arc<RwLock<HashMap<u64, Vec<VmSub>>>>,
    file_path: String,
}

impl SubStore {
    pub async fn load(path: &str) -> Self {
        let subs = if let Ok(data) = fs::read_to_string(path).await {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Self {
            subs: Arc::new(RwLock::new(subs)),
            file_path: path.to_string(),
        }
    }

    async fn save(&self) {
        let subs = self.subs.read().await;
        if let Ok(data) = serde_json::to_string_pretty(&*subs) {
            let _ = fs::write(&self.file_path, data).await;
        }
    }

    pub async fn add_sub(&self, user_id: u64, sub: VmSub) {
        {
            let mut subs = self.subs.write().await;
            subs.entry(user_id).or_default().push(sub);
        }
        self.save().await;
    }

    pub async fn remove_sub(&self, user_id: u64, index: usize) -> bool {
        let removed = {
            let mut subs = self.subs.write().await;
            if let Some(user_subs) = subs.get_mut(&user_id) {
                if index < user_subs.len() {
                    user_subs.remove(index);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };

        if removed {
            self.save().await;
        }
        removed
    }

    pub async fn get_subs(&self, user_id: u64) -> Vec<VmSub> {
        let subs = self.subs.read().await;
        subs.get(&user_id).cloned().unwrap_or_default()
    }

    pub async fn get_all(&self) -> HashMap<u64, Vec<VmSub>> {
        self.subs.read().await.clone()
    }
}
