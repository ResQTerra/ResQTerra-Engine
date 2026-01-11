//! Return-to-Home command handler

use super::HandlerContext;
use crate::command::CommandResult;
use resqterra_shared::{Command, DroneState, command, ReturnToHome};

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
    let rth_params = match &command.params {
        Some(command::Params::Rth(r)) => r.clone(),
        _ => {
            // RTH can work without explicit parameters (use defaults)
            println!("  [RTH] Using default parameters");
            ReturnToHome {
                altitude_m: 0.0,
                speed_mps: 0.0,
            }
        }
    };

    println!("  [RTH] Return-to-Home initiated");
    if rth_params.altitude_m > 0.0 {
        println!("    RTH altitude: {}m", rth_params.altitude_m);
    }
    if rth_params.speed_mps > 0.0 {
        println!("    RTH speed: {}m/s", rth_params.speed_mps);
    }

    // Dispatch via MAVLink
    match ctx.mav_cmd_sender.return_to_home(&rth_params).await {
        Ok(_) => CommandResult::Completed {
            message: "RTH initiated".into(),
        },
        Err(e) => CommandResult::Failed {
            message: format!("Failed to initiate RTH: {}", e),
        },
    }
}
