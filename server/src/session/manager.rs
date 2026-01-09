//! Session manager for tracking all connected drones

use super::connection::{DroneInfo, SessionHandle};
use resqterra_shared::{safety, Envelope};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Manages all active drone sessions
pub struct SessionManager {
    /// Map of device_id -> session handle
    sessions: Arc<RwLock<HashMap<String, SessionEntry>>>,
}

struct SessionEntry {
    handle: SessionHandle,
    info: DroneInfo,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new drone session
    pub async fn register(&self, handle: SessionHandle) {
        let device_id = handle.device_id.clone();
        if device_id.is_empty() {
            return; // Can't register without device ID
        }

        let info = DroneInfo::new(device_id.clone(), handle.addr);
        let entry = SessionEntry { handle, info };

        let mut sessions = self.sessions.write().await;
        sessions.insert(device_id, entry);
    }

    /// Unregister a drone session
    pub async fn unregister(&self, device_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(device_id);
    }

    /// Get a session handle for a specific drone
    pub async fn get(&self, device_id: &str) -> Option<SessionHandle> {
        let sessions = self.sessions.read().await;
        sessions.get(device_id).map(|e| e.handle.clone())
    }

    /// Send a message to a specific drone
    pub async fn send_to(&self, device_id: &str, envelope: &Envelope) -> anyhow::Result<()> {
        let handle = self.get(device_id).await
            .ok_or_else(|| anyhow::anyhow!("Drone not connected: {}", device_id))?;
        handle.send(envelope).await
    }

    /// Broadcast a message to all connected drones
    pub async fn broadcast(&self, envelope: &Envelope) {
        let sessions = self.sessions.read().await;
        for (device_id, entry) in sessions.iter() {
            if let Err(e) = entry.handle.send(envelope).await {
                eprintln!("Failed to send to {}: {}", device_id, e);
            }
        }
    }

    /// Get list of all connected device IDs
    pub async fn connected_devices(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    /// Get info about a specific drone
    pub async fn get_info(&self, device_id: &str) -> Option<DroneInfo> {
        let sessions = self.sessions.read().await;
        sessions.get(device_id).map(|e| e.info.clone())
    }

    /// Update drone info (e.g., from heartbeat)
    pub async fn update_heartbeat(&self, device_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(entry) = sessions.get_mut(device_id) {
            entry.info.last_heartbeat = Instant::now();
        }
    }

    /// Update drone state
    pub async fn update_state(&self, device_id: &str, state: resqterra_shared::DroneState) {
        let mut sessions = self.sessions.write().await;
        if let Some(entry) = sessions.get_mut(device_id) {
            entry.info.state = state;
        }
    }

    /// Check for dead sessions (heartbeat timeout)
    pub async fn check_dead_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        let timeout_ms = safety::HEARTBEAT_TIMEOUT_MS as u128;

        sessions
            .iter()
            .filter(|(_, entry)| entry.info.last_heartbeat.elapsed().as_millis() > timeout_ms)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Remove dead sessions and return their IDs
    pub async fn remove_dead_sessions(&self) -> Vec<String> {
        let dead = self.check_dead_sessions().await;
        if !dead.is_empty() {
            let mut sessions = self.sessions.write().await;
            for id in &dead {
                sessions.remove(id);
            }
        }
        dead
    }

    /// Get the number of connected drones
    pub async fn count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
