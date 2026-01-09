//! Return-to-Home command handler

use super::HandlerContext;
use crate::command::CommandResult;
use resqterra_shared::{Command, DroneState, command};

/// Handle RTH (Return-to-Home) command
///
/// RTH is a safety-critical command that should be accepted in almost any state
pub async fn handle_rth(ctx: &HandlerContext, command: &Command) -> CommandResult {
    // RTH is accepted in any flying state
    match ctx.current_state {
        DroneState::DroneIdle | DroneState::DronePreflight => {
            return CommandResult::Rejected {
                message: "Drone is not flying, RTH not needed".into(),
            };
        }
        DroneState::DroneReturningHome => {
            return CommandResult::Completed {
                message: "Already returning home".into(),
            };
        }
        DroneState::DroneLanding => {
            return CommandResult::Completed {
                message: "Already landing".into(),
            };
        }
        // Accept RTH in all other states (armed, taking off, in mission, emergency)
        _ => {}
    }

    // Extract RTH parameters
    let rth = match &command.params {
        Some(command::Params::Rth(r)) => r,
        _ => {
            // RTH can work without explicit parameters (use defaults)
            println!("  [RTH] Using default parameters");
            return CommandResult::Completed {
                message: "RTH initiated with defaults".into(),
            };
        }
    };

    println!("  [RTH] Return-to-Home initiated");
    if rth.altitude_m > 0.0 {
        println!("    RTH altitude: {}m", rth.altitude_m);
    }
    if rth.speed_mps > 0.0 {
        println!("    RTH speed: {}m/s", rth.speed_mps);
    }

    // TODO: In Phase 5, this will trigger RTL mode via MAVLink

    CommandResult::Completed {
        message: "RTH initiated".into(),
    }
}
