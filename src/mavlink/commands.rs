//! MAVLink Command Translation
//!
//! Translates ResQTerra commands to MAVLink commands for flight controller.

use anyhow::Result;
use mavlink::ardupilotmega::{
    MavCmd, MavMessage,
    COMMAND_LONG_DATA, MISSION_ITEM_INT_DATA, MavFrame,
};
use resqterra_shared::{MissionStart, ReturnToHome};
use tracing::{info, warn};

use super::connection::FlightController;

/// Sends commands to the flight controller via MAVLink
#[derive(Debug)]
pub struct MavCommandSender {
    fc: FlightController,
    target_system: u8,
    target_component: u8,
}

impl MavCommandSender {
    /// Create a new command sender
    pub fn new(fc: FlightController, target_system: u8, target_component: u8) -> Self {
        Self {
            fc,
            target_system,
            target_component,
        }
    }

    /// Return to home/launch position
    pub async fn return_to_home(&self, rth: &ReturnToHome) -> Result<()> {
        info!("[MAVLink] Sending RTL command");

        // Use COMMAND_LONG to set RTL mode
        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_DO_SET_MODE,
            confirmation: 0,
            param1: 1.0, // MAV_MODE_FLAG_CUSTOM_MODE_ENABLED
            param2: 6.0, // RTL mode for ArduPilot (mode number 6)
            param3: 0.0,
            param4: 0.0,
            param5: 0.0,
            param6: 0.0,
            param7: 0.0,
        });

        self.fc.send(msg).await?;

        // Optionally set RTL altitude if specified
        if rth.altitude_m > 0.0 {
            // This would require setting the RTL_ALT parameter
            // For now, we just use the default RTL altitude
            info!("[MAVLink] RTL altitude: {}m (using default)", rth.altitude_m);
        }

        Ok(())
    }

    /// Start a mission
    pub async fn start_mission(&self, mission: &MissionStart) -> Result<()> {
        info!("[MAVLink] Starting mission: {}", mission.mission_id);

        // First, upload mission waypoints
        if let Some(ref area) = mission.survey_area {
            self.upload_mission_waypoints(mission, area).await?;
        }

        // Then start the mission
        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_MISSION_START,
            confirmation: 0,
            param1: 0.0, // First waypoint
            param2: 0.0, // Last waypoint (0 = all)
            param3: 0.0,
            param4: 0.0,
            param5: 0.0,
            param6: 0.0,
            param7: 0.0,
        });

        self.fc.send(msg).await
    }

    /// Upload mission waypoints to flight controller
    async fn upload_mission_waypoints(
        &self,
        mission: &MissionStart,
        area: &resqterra_shared::SurveyArea,
    ) -> Result<()> {
        info!("[MAVLink] Uploading {} waypoints", area.boundary.len());

        // For a lawnmower pattern, we'd generate waypoints here
        // For now, just upload the boundary points as a simple mission

        for (i, point) in area.boundary.iter().enumerate() {
            let msg = MavMessage::MISSION_ITEM_INT(MISSION_ITEM_INT_DATA {
                target_system: self.target_system,
                target_component: self.target_component,
                seq: i as u16,
                frame: MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT_INT,
                command: MavCmd::MAV_CMD_NAV_WAYPOINT,
                current: if i == 0 { 1 } else { 0 },
                autocontinue: 1,
                param1: 0.0,  // Hold time
                param2: 2.0,  // Acceptance radius
                param3: 0.0,  // Pass through
                param4: 0.0,  // Yaw
                x: (point.latitude * 1e7) as i32,
                y: (point.longitude * 1e7) as i32,
                z: if point.altitude_m > 0.0 {
                    point.altitude_m
                } else {
                    mission.altitude_m
                },
            });

            self.fc.send(msg).await?;
        }

        Ok(())
    }

    /// Abort current mission
    pub async fn abort_mission(&self) -> Result<()> {
        info!("[MAVLink] Aborting mission - switching to LOITER");

        // Switch to LOITER mode (hold position) using COMMAND_LONG
        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_DO_SET_MODE,
            confirmation: 0,
            param1: 1.0, // MAV_MODE_FLAG_CUSTOM_MODE_ENABLED
            param2: 5.0, // LOITER mode for ArduPilot
            param3: 0.0,
            param4: 0.0,
            param5: 0.0,
            param6: 0.0,
            param7: 0.0,
        });

        self.fc.send(msg).await
    }

    /// Emergency stop - kills motors immediately
    pub async fn emergency_stop(&self) -> Result<()> {
        warn!("[MAVLink] EMERGENCY STOP - killing motors!");

        // Force disarm (even while flying - DANGEROUS!)
        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_COMPONENT_ARM_DISARM,
            confirmation: 0,
            param1: 0.0,    // 0 = disarm
            param2: 21196.0, // Magic number to force disarm while flying
            param3: 0.0,
            param4: 0.0,
            param5: 0.0,
            param6: 0.0,
            param7: 0.0,
        });

        self.fc.send(msg).await
    }

    /// Request status/data streams from FC
    pub async fn request_status(&self) -> Result<()> {
        info!("[MAVLink] Requesting data streams");

        // Request all data streams
        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_SET_MESSAGE_INTERVAL,
            confirmation: 0,
            param1: 33.0,   // GLOBAL_POSITION_INT
            param2: 100000.0, // 10 Hz (interval in microseconds)
            param3: 0.0,
            param4: 0.0,
            param5: 0.0,
            param6: 0.0,
            param7: 0.0,
        });

        self.fc.send(msg).await
    }
}