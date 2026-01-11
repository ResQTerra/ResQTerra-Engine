//! Mission command handlers (start, abort)

use super::HandlerContext;
use crate::command::CommandResult;
use resqterra_shared::{Command, DroneState, command};

/// Handle MISSION_START command
pub async fn handle_mission_start(ctx: &HandlerContext, command: &Command) -> CommandResult {
    // Validate state - can only start mission when ARMED or IDLE
    match ctx.current_state {
        DroneState::DroneArmed | DroneState::DroneIdle => {
            // Valid state to start mission
        }
        DroneState::DroneInMission => {
            return CommandResult::Rejected {
                message: "Already in mission".into(),
            };
        }
        DroneState::DroneReturningHome | DroneState::DroneLanding => {
            return CommandResult::Rejected {
                message: "Cannot start mission while returning/landing".into(),
            };
        }
        DroneState::DroneEmergency => {
            return CommandResult::Rejected {
                message: "Cannot start mission in emergency state".into(),
            };
        }
        _ => {
            return CommandResult::Rejected {
                message: format!("Invalid state for mission start: {:?}", ctx.current_state),
            };
        }
    }

    // Extract mission parameters
    let mission = match &command.params {
        Some(command::Params::MissionStart(m)) => m,
        _ => {
            return CommandResult::Rejected {
                message: "Missing mission parameters".into(),
            };
        }
    };

    println!("  [MISSION_START] Mission ID: {}", mission.mission_id);
    println!("    Altitude: {}m, Speed: {}m/s", mission.altitude_m, mission.speed_mps);
    println!("    Pattern: {:?}", resqterra_shared::ScanPattern::try_from(mission.scan_pattern).unwrap_or(resqterra_shared::ScanPattern::PatternUnknown));

    if let Some(ref area) = mission.survey_area {
        println!("    Survey area: {} boundary points", area.boundary.len());
        if let Some(ref home) = area.home_position {
            println!("    Home: lat={:.6}, lon={:.6}", home.latitude, home.longitude);
        }
    }

    // Dispatch via MAVLink
    match ctx.mav_cmd_sender.start_mission(mission).await {
        Ok(_) => CommandResult::Completed {
            message: format!("Mission {} started", mission.mission_id),
        },
        Err(e) => CommandResult::Failed {
            message: format!("Failed to start mission: {}", e),
        },
    }
}

/// Handle MISSION_ABORT command
pub async fn handle_mission_abort(ctx: &HandlerContext, command: &Command) -> CommandResult {
    // Can only abort if in mission
    if ctx.current_state != DroneState::DroneInMission {
        return CommandResult::Rejected {
            message: format!("Not in mission (state: {:?})", ctx.current_state),
        };
    }

    // Extract abort parameters
    let abort = match &command.params {
        Some(command::Params::MissionAbort(a)) => a,
        _ => {
            return CommandResult::Rejected {
                message: "Missing abort parameters".into(),
            };
        }
    };

    let action = resqterra_shared::AbortAction::try_from(abort.action)
        .unwrap_or(resqterra_shared::AbortAction::AbortHover);

    println!("  [MISSION_ABORT] Reason: {}", abort.reason);
    println!("    Action: {:?}", action);

    // Dispatch via MAVLink
    match ctx.mav_cmd_sender.abort_mission().await {
        Ok(_) => CommandResult::Completed {
            message: format!("Mission aborted: {}", abort.reason),
        },
        Err(e) => CommandResult::Failed {
            message: format!("Failed to abort mission: {}", e),
        },
    }
}
