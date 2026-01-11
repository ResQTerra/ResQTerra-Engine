//! Status request command handler

use super::HandlerContext;
use crate::command::CommandResult;
use resqterra_shared::Command;

/// Handle STATUS_REQUEST command
pub async fn handle_status_request(ctx: &HandlerContext, _command: &Command) -> CommandResult {
    // Status request is always valid regardless of state
    // In a real implementation, this would gather telemetry and send it back

    println!("  [STATUS_REQUEST] Gathering status for {}", ctx.device_id);
    println!("    Current state: {:?}", ctx.current_state);

    // Dispatch via MAVLink
    match ctx.mav_cmd_sender.request_status().await {
        Ok(_) => CommandResult::Completed {
            message: format!("Status request sent. Current state: {:?}", ctx.current_state),
        },
        Err(e) => CommandResult::Failed {
            message: format!("Failed to request status: {}", e),
        },
    }
}
