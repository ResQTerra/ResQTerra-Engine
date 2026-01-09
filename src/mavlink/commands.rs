//! MAVLink Command Translation
//!
//! Translates ResQTerra commands to MAVLink commands for flight controller.

use anyhow::Result;
use mavlink::ardupilotmega::{
    MavCmd, MavFrame, MavMessage,
    COMMAND_LONG_DATA, MISSION_ITEM_INT_DATA,
};
use resqterra_shared::{Command, CommandType, MissionStart, ReturnToHome};

use super::connection::FlightController;

/// Sends commands to the flight controller via MAVLink
pub struct MavCommandSender {
    target_system: u8,
    target_component: u8,
}

impl MavCommandSender {
    /// Create a new command sender
    pub fn new(target_system: u8, target_component: u8) -> Self {
        Self {
            target_system,
            target_component,
        }
    }

    /// Translate and send a ResQTerra command to the flight controller
    pub async fn send_command(&self, fc: &FlightController, command: &Command) -> Result<()> {
        let cmd_type = CommandType::try_from(command.cmd_type).unwrap_or(CommandType::CmdUnknown);

        match cmd_type {
            CommandType::CmdMissionStart => {
                if let Some(resqterra_shared::command::Params::MissionStart(mission)) =
                    &command.params
                {
                    self.start_mission(fc, mission).await?;
                }
            }
            CommandType::CmdMissionAbort => {
                self.abort_mission(fc).await?;
            }
            CommandType::CmdRth => {
                if let Some(resqterra_shared::command::Params::Rth(rth)) = &command.params {
                    self.return_to_home(fc, rth).await?;
                } else {
                    // Default RTH with zero values (use FC defaults)
                    self.return_to_home(fc, &ReturnToHome {
                        altitude_m: 0.0,
                        speed_mps: 0.0,
                    }).await?;
                }
            }
            CommandType::CmdEmergencyStop => {
                self.emergency_stop(fc).await?;
            }
            CommandType::CmdStatusRequest => {
                self.request_status(fc).await?;
            }
            _ => {
                println!("[MAVLink] Unknown command type: {:?}", cmd_type);
            }
        }

        Ok(())
    }

    /// Arm the drone
    pub async fn arm(&self, fc: &FlightController) -> Result<()> {
        println!("[MAVLink] Sending ARM command");

        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_COMPONENT_ARM_DISARM,
            confirmation: 0,
            param1: 1.0, // 1 = arm
            param2: 0.0,
            param3: 0.0,
            param4: 0.0,
            param5: 0.0,
            param6: 0.0,
            param7: 0.0,
        });

        fc.send(msg).await
    }

    /// Disarm the drone
    pub async fn disarm(&self, fc: &FlightController) -> Result<()> {
        println!("[MAVLink] Sending DISARM command");

        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_COMPONENT_ARM_DISARM,
            confirmation: 0,
            param1: 0.0, // 0 = disarm
            param2: 0.0,
            param3: 0.0,
            param4: 0.0,
            param5: 0.0,
            param6: 0.0,
            param7: 0.0,
        });

        fc.send(msg).await
    }

    /// Take off to specified altitude
    pub async fn takeoff(&self, fc: &FlightController, altitude_m: f32) -> Result<()> {
        println!("[MAVLink] Sending TAKEOFF to {}m", altitude_m);

        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_NAV_TAKEOFF,
            confirmation: 0,
            param1: 0.0,        // Minimum pitch
            param2: 0.0,        // Empty
            param3: 0.0,        // Empty
            param4: f32::NAN,   // Yaw angle (NAN = current)
            param5: f32::NAN,   // Latitude (NAN = current)
            param6: f32::NAN,   // Longitude (NAN = current)
            param7: altitude_m, // Altitude
        });

        fc.send(msg).await
    }

    /// Land at current position
    pub async fn land(&self, fc: &FlightController) -> Result<()> {
        println!("[MAVLink] Sending LAND command");

        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_NAV_LAND,
            confirmation: 0,
            param1: 0.0,      // Abort altitude
            param2: 0.0,      // Land mode
            param3: 0.0,      // Empty
            param4: f32::NAN, // Yaw angle
            param5: f32::NAN, // Latitude
            param6: f32::NAN, // Longitude
            param7: 0.0,      // Altitude
        });

        fc.send(msg).await
    }

    /// Return to home/launch position
    pub async fn return_to_home(&self, fc: &FlightController, rth: &ReturnToHome) -> Result<()> {
        println!("[MAVLink] Sending RTL command");

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

        fc.send(msg).await?;

        // Optionally set RTL altitude if specified
        if rth.altitude_m > 0.0 {
            // This would require setting the RTL_ALT parameter
            // For now, we just use the default RTL altitude
            println!("[MAVLink] RTL altitude: {}m (using default)", rth.altitude_m);
        }

        Ok(())
    }

    /// Start a mission
    pub async fn start_mission(&self, fc: &FlightController, mission: &MissionStart) -> Result<()> {
        println!("[MAVLink] Starting mission: {}", mission.mission_id);

        // First, upload mission waypoints
        if let Some(ref area) = mission.survey_area {
            self.upload_mission_waypoints(fc, mission, area).await?;
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

        fc.send(msg).await
    }

    /// Upload mission waypoints to flight controller
    async fn upload_mission_waypoints(
        &self,
        fc: &FlightController,
        mission: &MissionStart,
        area: &resqterra_shared::SurveyArea,
    ) -> Result<()> {
        println!("[MAVLink] Uploading {} waypoints", area.boundary.len());

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

            fc.send(msg).await?;
        }

        Ok(())
    }

    /// Abort current mission
    pub async fn abort_mission(&self, fc: &FlightController) -> Result<()> {
        println!("[MAVLink] Aborting mission - switching to LOITER");

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

        fc.send(msg).await
    }

    /// Emergency stop - kills motors immediately
    pub async fn emergency_stop(&self, fc: &FlightController) -> Result<()> {
        println!("[MAVLink] EMERGENCY STOP - killing motors!");

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

        fc.send(msg).await
    }

    /// Request status/data streams from FC
    pub async fn request_status(&self, fc: &FlightController) -> Result<()> {
        println!("[MAVLink] Requesting data streams");

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

        fc.send(msg).await
    }

    /// Set flight mode
    pub async fn set_mode(&self, fc: &FlightController, mode: ArduPilotMode) -> Result<()> {
        println!("[MAVLink] Setting mode to {:?}", mode);

        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            command: MavCmd::MAV_CMD_DO_SET_MODE,
            confirmation: 0,
            param1: 1.0, // MAV_MODE_FLAG_CUSTOM_MODE_ENABLED
            param2: mode as u32 as f32,
            param3: 0.0,
            param4: 0.0,
            param5: 0.0,
            param6: 0.0,
            param7: 0.0,
        });

        fc.send(msg).await
    }

    /// Go to a specific GPS position
    pub async fn goto_position(
        &self,
        fc: &FlightController,
        lat: f64,
        lon: f64,
        alt: f32,
    ) -> Result<()> {
        println!(
            "[MAVLink] Going to position: lat={:.6}, lon={:.6}, alt={}m",
            lat, lon, alt
        );

        let msg = MavMessage::MISSION_ITEM_INT(MISSION_ITEM_INT_DATA {
            target_system: self.target_system,
            target_component: self.target_component,
            seq: 0,
            frame: MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT_INT,
            command: MavCmd::MAV_CMD_NAV_WAYPOINT,
            current: 2, // Guided mode waypoint
            autocontinue: 0,
            param1: 0.0,
            param2: 0.0,
            param3: 0.0,
            param4: 0.0,
            x: (lat * 1e7) as i32,
            y: (lon * 1e7) as i32,
            z: alt,
        });

        fc.send(msg).await
    }
}

/// ArduPilot Copter flight modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ArduPilotMode {
    Stabilize = 0,
    Acro = 1,
    AltHold = 2,
    Auto = 3,
    Guided = 4,
    Loiter = 5,
    Rtl = 6,
    Circle = 7,
    Land = 9,
    Drift = 11,
    Sport = 13,
    Flip = 14,
    AutoTune = 15,
    PosHold = 16,
    Brake = 17,
    Throw = 18,
    AvoidAdsb = 19,
    GuidedNoGps = 20,
    SmartRtl = 21,
    FlowHold = 22,
    Follow = 23,
    ZigZag = 24,
    SystemId = 25,
    HeliAutorotate = 26,
    AutoRtl = 27,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ardupilot_modes() {
        assert_eq!(ArduPilotMode::Guided as u32, 4);
        assert_eq!(ArduPilotMode::Rtl as u32, 6);
        assert_eq!(ArduPilotMode::Land as u32, 9);
    }
}
