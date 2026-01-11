//! Command executor - validates and dispatches incoming commands

use super::handlers::{self, HandlerContext};
use crate::mavlink::MavCommandSender;
use resqterra_shared::{
    Ack, AckStatus, Command, CommandType, DroneState, Envelope, Header, MessageType, now_ms,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Result of command execution
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// Command accepted and completed successfully
    Completed { message: String },
    /// Command accepted but execution failed
    Failed { message: String },
    /// Command rejected (invalid state, expired, etc.)
    Rejected { message: String },
}

/// Executes commands received from the server
pub struct CommandExecutor {
    device_id: String,
    sequence_id: Arc<AtomicU64>,
    current_state: Arc<RwLock<DroneState>>,
    mav_cmd_sender: Arc<MavCommandSender>,
}

impl CommandExecutor {
    /// Create a new command executor
    pub fn new(
        device_id: String,
        sequence_id: Arc<AtomicU64>,
        mav_cmd_sender: Arc<MavCommandSender>,
    ) -> Self {
        Self {
            device_id,
            sequence_id,
            current_state: Arc::new(RwLock::new(DroneState::DroneIdle)),
            mav_cmd_sender,
        }
    }

    /// Get the current drone state
    pub async fn get_state(&self) -> DroneState {
        *self.current_state.read().await
    }

    /// Execute a command and return the appropriate ACK envelope
    pub async fn execute(&self, command: &Command, header: &Header) -> Envelope {
        let start_time = now_ms();
        let cmd_type = CommandType::try_from(command.cmd_type).unwrap_or(CommandType::CmdUnknown);

        info!(
            "Executing command: id={} type={:?}",
            command.command_id, cmd_type
        );

        // Check if command has expired
        if command.expires_at_ms > 0 && now_ms() > command.expires_at_ms {
            warn!("  Command expired");
            return self.create_ack(
                header.sequence_id,
                command.command_id,
                AckStatus::AckExpired,
                "Command expired before execution",
                0,
            );
        }

        // Create handler context
        let ctx = HandlerContext {
            device_id: self.device_id.clone(),
            current_state: self.get_state().await,
            mav_cmd_sender: self.mav_cmd_sender.clone(),
        };

        // Dispatch to appropriate handler
        let result = match cmd_type {
            CommandType::CmdStatusRequest => {
                handlers::handle_status_request(&ctx, command).await
            }
            CommandType::CmdMissionStart => {
                handlers::handle_mission_start(&ctx, command).await
            }
            CommandType::CmdMissionAbort => {
                handlers::handle_mission_abort(&ctx, command).await
            }
            CommandType::CmdRth => {
                handlers::handle_rth(&ctx, command).await
            }
            CommandType::CmdConfigUpdate => {
                handlers::handle_config_update(&ctx, command).await
            }
            CommandType::CmdEmergencyStop => {
                handlers::handle_emergency_stop(&ctx, command).await
            }
            CommandType::CmdUnknown => {
                CommandResult::Rejected {
                    message: "Unknown command type".into(),
                }
            }
        };

        let processing_time = now_ms() - start_time;

        // Convert result to ACK
        match result {
            CommandResult::Completed { message } => {
                info!("  Command completed: {}", message);
                self.create_ack(
                    header.sequence_id,
                    command.command_id,
                    AckStatus::AckCompleted,
                    &message,
                    processing_time,
                )
            }
            CommandResult::Failed { message } => {
                error!("  Command failed: {}", message);
                self.create_ack(
                    header.sequence_id,
                    command.command_id,
                    AckStatus::AckFailed,
                    &message,
                    processing_time,
                )
            }
            CommandResult::Rejected { message } => {
                warn!("  Command rejected: {}", message);
                self.create_ack(
                    header.sequence_id,
                    command.command_id,
                    AckStatus::AckRejected,
                    &message,
                    processing_time,
                )
            }
        }
    }

    /// Create an ACK envelope
    fn create_ack(
        &self,
        ack_sequence_id: u64,
        command_id: u64,
        status: AckStatus,
        message: &str,
        processing_time_ms: u64,
    ) -> Envelope {
        let seq = self.sequence_id.fetch_add(1, Ordering::SeqCst) + 1;

        Envelope {
            header: Some(Header::new(&self.device_id, MessageType::MsgAck, seq)),
            payload: Some(resqterra_shared::envelope::Payload::Ack(Ack {
                ack_sequence_id,
                command_id,
                status: status.into(),
                message: message.into(),
                processing_time_ms,
            })),
        }
    }
}
