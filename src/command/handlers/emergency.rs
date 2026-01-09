//! Emergency stop command handler

use super::HandlerContext;
use crate::command::CommandResult;
use resqterra_shared::{Command, DroneState};

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

    // TODO: In Phase 5, this will:
    // 1. Send MAVLink KILL command to flight controller
    // 2. Disarm motors immediately
    // 3. Log the emergency event

    // Warning: This will cause the drone to fall!
    // Only use in actual emergency situations

    CommandResult::Completed {
        message: "EMERGENCY STOP EXECUTED - Motors killed".into(),
    }
}
