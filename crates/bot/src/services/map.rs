use crate::Data;
use rustplus::proto::{AppMarker, AppMarkerType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MapService {
    // Cache for map size (map_size doesn't change during a wipe)
    map_sizes: Arc<Mutex<HashMap<i32, u32>>>,
}

impl MapService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            map_sizes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Gets the map size for a server, fetching it if not cached.
    ///
    /// # Errors
    /// Returns an error if the server is not connected or API fails.
    pub async fn get_map_size(&self, server_id: i32, data: &Data) -> anyhow::Result<u32> {
        let mut lock = self.map_sizes.lock().await;
        if let Some(&size) = lock.get(&server_id) {
            return Ok(size);
        }

        let clients = data.rustplus_clients.lock().await;
        let client = clients
            .get(&server_id)
            .ok_or_else(|| anyhow::anyhow!("Server not connected"))?;

        let info_msg = client.get_info().await?;
        let size = info_msg
            .response
            .and_then(|r| r.info)
            .map_or(4000, |i| i.map_size);

        lock.insert(server_id, size);
        Ok(size)
    }

    /// Fetches all map markers for a server.
    ///
    /// # Errors
    /// Returns an error if the server is not connected or API fails.
    pub async fn get_markers(&self, server_id: i32, data: &Data) -> anyhow::Result<Vec<AppMarker>> {
        let clients = data.rustplus_clients.lock().await;
        let client = clients
            .get(&server_id)
            .ok_or_else(|| anyhow::anyhow!("Server not connected"))?;

        let markers_msg = client.get_map_markers().await?;
        let markers = markers_msg
            .response
            .and_then(|r| r.map_markers)
            .map_or(vec![], |m| m.markers);

        Ok(markers)
    }

    /// Fetches only vending machines for a server.
    ///
    /// # Errors
    /// Returns an error if fetching markers fails.
    pub async fn get_vending_machines(
        &self,
        server_id: i32,
        data: &Data,
    ) -> anyhow::Result<Vec<AppMarker>> {
        let markers = self.get_markers(server_id, data).await?;
        let vending_machines = markers
            .into_iter()
            .filter(|m| m.r#type == i32::from(AppMarkerType::VendingMachine))
            .collect();

        Ok(vending_machines)
    }
}

impl Default for MapService {
    fn default() -> Self {
        Self::new()
    }
}
