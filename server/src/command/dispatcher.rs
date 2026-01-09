//! Command dispatcher for sending commands to drones

use crate::session::SessionManager;
use resqterra_shared::{
    envelope, Command, CommandType, Envelope, Header, MessageType, now_ms, safety,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Tracks a sent command awaiting response
#[derive(Debug, Clone)]
pub struct PendingCommand {
    pub command_id: u64,
    pub sequence_id: u64,
    pub device_id: String,
    pub cmd_type: CommandType,
    pub sent_at: u64,
    pub expires_at: u64,
    pub retries: u32,
    pub max_retries: u32,
}

impl PendingCommand {
    /// Check if this command has timed out (ACK not received)
    pub fn is_timed_out(&self) -> bool {
        let timeout_at = self.sent_at + safety::COMMAND_ACK_TIMEOUT_MS;
        now_ms() > timeout_at
    }

    /// Check if this command has expired (too old to execute)
    pub fn is_expired(&self) -> bool {
        self.expires_at > 0 && now_ms() > self.expires_at
    }

    /// Check if this command can be retried
    pub fn can_retry(&self) -> bool {
        self.retries < self.max_retries && !self.is_expired()
    }
}

/// Dispatches commands to drones and tracks responses
pub struct CommandDispatcher {
    session_manager: Arc<SessionManager>,
    sequence_id: Arc<AtomicU64>,
    command_id: Arc<AtomicU64>,
    /// Pending commands by command_id
    pending: Arc<RwLock<HashMap<u64, PendingCommand>>>,
}

impl CommandDispatcher {
    /// Create a new command dispatcher
    pub fn new(session_manager: Arc<SessionManager>, sequence_id: Arc<AtomicU64>) -> Self {
        Self {
            session_manager,
            sequence_id,
            command_id: Arc::new(AtomicU64::new(0)),
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the next command ID
    pub fn next_command_id(&self) -> u64 {
        self.command_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Get the next sequence ID
    pub fn next_sequence_id(&self) -> u64 {
        self.sequence_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Send a command to a specific drone
    pub async fn send_command(
        &self,
        device_id: &str,
        command: Command,
    ) -> anyhow::Result<u64> {
        let seq = self.next_sequence_id();
        let cmd_id = command.command_id;
        let cmd_type = CommandType::try_from(command.cmd_type).unwrap_or(CommandType::CmdUnknown);

        let envelope = Envelope {
            header: Some(Header::new("server", MessageType::MsgCommand, seq)),
            payload: Some(envelope::Payload::Command(command.clone())),
        };

        // Track pending command
        let pending = PendingCommand {
            command_id: cmd_id,
            sequence_id: seq,
            device_id: device_id.to_string(),
            cmd_type,
            sent_at: now_ms(),
            expires_at: command.expires_at_ms,
            retries: 0,
            max_retries: safety::COMMAND_MAX_RETRIES,
        };

        self.pending.write().await.insert(cmd_id, pending);

        // Send to drone
        self.session_manager.send_to(device_id, &envelope).await?;

        println!(
            ">>> Sent command {} ({:?}) to {} (seq={})",
            cmd_id, cmd_type, device_id, seq
        );

        Ok(cmd_id)
    }

    /// Broadcast a command to all connected drones
    pub async fn broadcast_command(&self, mut command: Command) -> Vec<u64> {
        let devices = self.session_manager.connected_devices().await;
        let mut command_ids = Vec::new();

        for device_id in devices {
            // Each drone gets a unique command_id
            command.command_id = self.next_command_id();

            match self.send_command(&device_id, command.clone()).await {
                Ok(cmd_id) => command_ids.push(cmd_id),
                Err(e) => eprintln!("Failed to send to {}: {}", device_id, e),
            }
        }

        command_ids
    }

    /// Handle an ACK received from a drone
    pub async fn handle_ack(&self, device_id: &str, ack: &resqterra_shared::Ack) {
        let status = resqterra_shared::AckStatus::try_from(ack.status)
            .unwrap_or(resqterra_shared::AckStatus::AckUnknown);

        let mut pending = self.pending.write().await;

        if let Some(cmd) = pending.get(&ack.command_id) {
            println!(
                "<<< ACK for command {} from {}: {:?} ({}ms)",
                ack.command_id, device_id, status, ack.processing_time_ms
            );

            match status {
                resqterra_shared::AckStatus::AckCompleted
                | resqterra_shared::AckStatus::AckFailed
                | resqterra_shared::AckStatus::AckRejected
                | resqterra_shared::AckStatus::AckExpired => {
                    // Command is done, remove from pending
                    pending.remove(&ack.command_id);
                }
                resqterra_shared::AckStatus::AckReceived
                | resqterra_shared::AckStatus::AckAccepted => {
                    // Command is being processed, keep tracking
                    println!("    Command {} is being processed", ack.command_id);
                }
                _ => {}
            }

            if !ack.message.is_empty() {
                println!("    Message: {}", ack.message);
            }
        } else {
            println!(
                "<<< ACK for unknown command {} from {}",
                ack.command_id, device_id
            );
        }
    }

    /// Get timed out commands that need retry or failure handling
    pub async fn get_timed_out_commands(&self) -> Vec<PendingCommand> {
        let pending = self.pending.read().await;
        pending
            .values()
            .filter(|c| c.is_timed_out())
            .cloned()
            .collect()
    }

    /// Retry a timed out command
    pub async fn retry_command(&self, command_id: u64) -> anyhow::Result<()> {
        let mut pending = self.pending.write().await;

        if let Some(cmd) = pending.get_mut(&command_id) {
            if !cmd.can_retry() {
                pending.remove(&command_id);
                return Err(anyhow::anyhow!(
                    "Command {} exceeded max retries or expired",
                    command_id
                ));
            }

            cmd.retries += 1;
            cmd.sent_at = now_ms();

            println!(
                ">>> Retrying command {} (attempt {}/{})",
                command_id,
                cmd.retries + 1,
                cmd.max_retries + 1
            );

            // TODO: Re-send the actual command
            // For now, we just update the tracking
        }

        Ok(())
    }

    /// Remove expired commands
    pub async fn cleanup_expired(&self) -> Vec<u64> {
        let mut pending = self.pending.write().await;
        let expired: Vec<u64> = pending
            .iter()
            .filter(|(_, c)| c.is_expired())
            .map(|(id, _)| *id)
            .collect();

        for id in &expired {
            pending.remove(id);
            println!("Command {} expired and removed", id);
        }

        expired
    }

    /// Get count of pending commands
    pub async fn pending_count(&self) -> usize {
        self.pending.read().await.len()
    }

    /// Get count of pending commands for a specific drone
    pub async fn pending_count_for(&self, device_id: &str) -> usize {
        self.pending
            .read()
            .await
            .values()
            .filter(|c| c.device_id == device_id)
            .count()
    }
}
