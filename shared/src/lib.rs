//! ResQTerra Shared Protocol Types
//!
//! This crate provides the shared protocol types and codec for communication
//! between drone edge devices, relay nodes, and the server.

pub mod codec;
pub mod state_machine;

use std::time::{SystemTime, UNIX_EPOCH};

// Include the generated protobuf types
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/resqterra.rs"));
}

// Re-export commonly used types at crate root
pub use proto::*;

/// Get current timestamp in milliseconds since Unix epoch
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Safety parameters for the system
pub mod safety {
    /// Heartbeat interval in milliseconds
    pub const HEARTBEAT_INTERVAL_MS: u64 = 1000;

    /// Heartbeat timeout - triggers RTH if no heartbeat received
    pub const HEARTBEAT_TIMEOUT_MS: u64 = 10000;

    /// Command ACK timeout in milliseconds
    pub const COMMAND_ACK_TIMEOUT_MS: u64 = 3000;

    /// Maximum command retries before giving up
    pub const COMMAND_MAX_RETRIES: u32 = 3;

    /// Maximum age for a command before it's considered expired
    pub const COMMAND_MAX_AGE_MS: u64 = 30000;

    /// Critical battery percentage - triggers forced RTH
    pub const BATTERY_CRITICAL_PERCENT: u32 = 20;
}

/// Builder helpers for creating messages
impl Header {
    /// Create a new header with the given device ID and message type
    pub fn new(device_id: impl Into<String>, msg_type: MessageType, sequence_id: u64) -> Self {
        Self {
            device_id: device_id.into(),
            sequence_id,
            timestamp_ms: now_ms(),
            msg_type: msg_type.into(),
        }
    }
}

impl Heartbeat {
    /// Create a new heartbeat message
    pub fn new(uptime_ms: u64, state: DroneState, pending_commands: u32, healthy: bool) -> Self {
        Self {
            uptime_ms,
            state: state.into(),
            pending_commands,
            healthy,
        }
    }
}

impl Ack {
    /// Create an ACK for a received command
    pub fn received(sequence_id: u64, command_id: u64) -> Self {
        Self {
            ack_sequence_id: sequence_id,
            command_id,
            status: AckStatus::AckReceived.into(),
            message: String::new(),
            processing_time_ms: 0,
        }
    }

    /// Create an ACK for a completed command
    pub fn completed(sequence_id: u64, command_id: u64, processing_time_ms: u64) -> Self {
        Self {
            ack_sequence_id: sequence_id,
            command_id,
            status: AckStatus::AckCompleted.into(),
            message: String::new(),
            processing_time_ms,
        }
    }

    /// Create an ACK for a failed command
    pub fn failed(sequence_id: u64, command_id: u64, message: impl Into<String>) -> Self {
        Self {
            ack_sequence_id: sequence_id,
            command_id,
            status: AckStatus::AckFailed.into(),
            message: message.into(),
            processing_time_ms: 0,
        }
    }

    /// Create an ACK for a rejected command
    pub fn rejected(sequence_id: u64, command_id: u64, message: impl Into<String>) -> Self {
        Self {
            ack_sequence_id: sequence_id,
            command_id,
            status: AckStatus::AckRejected.into(),
            message: message.into(),
            processing_time_ms: 0,
        }
    }

    /// Create an ACK for an expired command
    pub fn expired(sequence_id: u64, command_id: u64) -> Self {
        Self {
            ack_sequence_id: sequence_id,
            command_id,
            status: AckStatus::AckExpired.into(),
            message: "Command expired".into(),
            processing_time_ms: 0,
        }
    }
}

impl Command {
    /// Check if this command has expired
    pub fn is_expired(&self) -> bool {
        if self.expires_at_ms == 0 {
            return false; // No expiry set
        }
        now_ms() > self.expires_at_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_creation() {
        let header = Header::new("edge-001", MessageType::MsgHeartbeat, 1);
        assert_eq!(header.device_id, "edge-001");
        assert_eq!(header.sequence_id, 1);
        assert!(header.timestamp_ms > 0);
    }

    #[test]
    fn test_heartbeat_creation() {
        let hb = Heartbeat::new(1000, DroneState::DroneIdle, 0, true);
        assert_eq!(hb.uptime_ms, 1000);
        assert!(hb.healthy);
    }

    #[test]
    fn test_ack_creation() {
        let ack = Ack::completed(1, 100, 50);
        assert_eq!(ack.ack_sequence_id, 1);
        assert_eq!(ack.command_id, 100);
        assert_eq!(ack.processing_time_ms, 50);
    }
}
