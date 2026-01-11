//! Emergency stop command handler

use super::HandlerContext;
use crate::command::CommandResult;
use resqterra_shared::Command;

/// Handle EMERGENCY_STOP command
///
/// This is the highest priority command - immediately stops all motors.
/// USE WITH EXTREME CAUTION - drone will fall from sky!
pub async fn handle_emergency_stop(ctx: &HandlerContext, _command: &Command) -> CommandResult {
    println!("  [EMERGENCY_STOP] !!!!!!!!!!!!!!!!!!!!!!!!");
    println!("  [EMERGENCY_STOP] EMERGENCY STOP TRIGGERED");
    println!("  [EMERGENCY_STOP] Current state: {:?}", ctx.current_state);
    println!("  [EMERGENCY_STOP] !!!!!!!!!!!!!!!!!!!!!!!!");

    // Emergency stop is ALWAYS accepted, regardless of state
    // This is a safety feature - if something goes wrong, we need to be able to stop

    // Dispatch via MAVLink
    match ctx.mav_cmd_sender.emergency_stop().await {
        Ok(_) => CommandResult::Completed {
            message: "EMERGENCY STOP EXECUTED - Motors killed".into(),
        },
        Err(e) => CommandResult::Failed {
            message: format!("Failed to execute emergency stop: {}", e),
        },
    }
}
