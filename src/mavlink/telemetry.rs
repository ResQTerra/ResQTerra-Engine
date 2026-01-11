//! MAVLink Telemetry Reader
//!
//! Reads telemetry from flight controller and converts to ResQTerra format.

use mavlink::ardupilotmega::MavMessage;
use resqterra_shared::{
    BatteryStatus, ConnectionQuality, DroneState, FlightControllerStatus, GpsPosition, Telemetry,
    Transport,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Reads and converts MAVLink telemetry to ResQTerra format
pub struct TelemetryReader {
    /// Latest GPS position
    position: Arc<RwLock<Option<GpsPosition>>>,
    /// Latest battery status
    battery: Arc<RwLock<Option<BatteryStatus>>>,
    /// Latest FC status
    fc_status: Arc<RwLock<FlightControllerStatus>>,
    /// Current drone state
    state: Arc<RwLock<DroneState>>,
    /// Start time for calculating uptime
    start_time: std::time::Instant,
}

impl TelemetryReader {
    /// Create a new telemetry reader
    pub fn new() -> Self {
        Self {
            position: Arc::new(RwLock::new(None)),
            battery: Arc::new(RwLock::new(None)),
            fc_status: Arc::new(RwLock::new(FlightControllerStatus {
                armed: false,
                gps_lock: false,
                mode: String::new(),
                error_count: 0,
                active_faults: vec![],
            })),
            state: Arc::new(RwLock::new(DroneState::DroneIdle)),
            start_time: std::time::Instant::now(),
        }
    }

    /// Process a MAVLink message and update telemetry
    pub async fn process_message(&self, msg: &MavMessage) {
        match msg {
            MavMessage::GLOBAL_POSITION_INT(pos) => {
                let gps = GpsPosition {
                    latitude: pos.lat as f64 / 1e7,
                    longitude: pos.lon as f64 / 1e7,
                    altitude_m: pos.alt as f32 / 1000.0, // mm to m
                    heading_deg: pos.hdg as f32 / 100.0, // cdeg to deg
                    ground_speed_mps: ((pos.vx.pow(2) + pos.vy.pow(2)) as f32).sqrt() / 100.0,
                    satellites: 0, // Not in this message
                    hdop: 0.0,
                };
                *self.position.write().await = Some(gps);
            }

            MavMessage::GPS_RAW_INT(gps) => {
                // Update satellite count and HDOP
                if let Some(ref mut pos) = *self.position.write().await {
                    pos.satellites = gps.satellites_visible as u32;
                    pos.hdop = gps.eph as f32 / 100.0;
                }

                // Update GPS lock status
                let mut fc = self.fc_status.write().await;
                fc.gps_lock = gps.fix_type as u8 >= 3; // 3D fix or better
            }

            MavMessage::SYS_STATUS(sys) => {
                let battery = BatteryStatus {
                    voltage: sys.voltage_battery as f32 / 1000.0, // mV to V
                    current: sys.current_battery as f32 / 100.0,  // cA to A
                    remaining_percent: sys.battery_remaining as u32,
                    remaining_seconds: 0, // Not provided by SYS_STATUS
                };
                *self.battery.write().await = Some(battery);

                // Update error count
                let mut fc = self.fc_status.write().await;
                fc.error_count = sys.errors_count1 as u32
                    + sys.errors_count2 as u32
                    + sys.errors_count3 as u32
                    + sys.errors_count4 as u32;
            }

            MavMessage::BATTERY_STATUS(bat) => {
                if let Some(ref mut battery) = *self.battery.write().await {
                    battery.remaining_percent = bat.battery_remaining as u32;
                    // Calculate remaining time if current is known
                    if battery.current > 0.1 {
                        // Rough estimate based on capacity and current
                        let capacity_mah = bat.current_consumed as f32;
                        battery.remaining_seconds =
                            ((capacity_mah / 1000.0) / battery.current * 3600.0) as u32;
                    }
                }
            }

            MavMessage::HEARTBEAT(hb) => {
                // Update armed status
                let armed = (hb.base_mode.bits() & 0x80) != 0; // MAV_MODE_FLAG_SAFETY_ARMED

                let mut fc = self.fc_status.write().await;
                fc.armed = armed;
                fc.mode = mode_to_string(hb.custom_mode);

                // Update drone state based on mode
                drop(fc);
                self.update_state_from_mode(hb.custom_mode, armed).await;
            }

            MavMessage::STATUSTEXT(text) => {
                // Log status text and check for faults
                let text_str = String::from_utf8_lossy(&text.text).to_string();
                let text_str = text_str.trim_end_matches('\0');

                if text.severity as u8 <= 3 {
                    // EMERGENCY, ALERT, CRITICAL, ERROR
                    let mut fc = self.fc_status.write().await;
                    fc.active_faults.push(text_str.to_string());

                    // Keep only last 10 faults
                    if fc.active_faults.len() > 10 {
                        fc.active_faults.remove(0);
                    }
                }

                println!("[FC] {}: {}", severity_to_string(text.severity as u8), text_str);
            }

            MavMessage::VFR_HUD(hud) => {
                // Update ground speed if available
                if let Some(ref mut pos) = *self.position.write().await {
                    pos.ground_speed_mps = hud.groundspeed;
                    pos.heading_deg = hud.heading as f32;
                }
            }

            _ => {
                // Other messages we don't process
            }
        }
    }

    /// Update drone state based on flight mode
    async fn update_state_from_mode(&self, custom_mode: u32, armed: bool) {
        let new_state = match custom_mode {
            6 => DroneState::DroneReturningHome, // RTL
            9 => DroneState::DroneLanding,       // LAND
            3 => DroneState::DroneInMission,     // AUTO
            4 => DroneState::DroneInMission,     // GUIDED
            _ if !armed => DroneState::DroneIdle,
            _ => DroneState::DroneArmed,
        };

        *self.state.write().await = new_state;
    }

    /// Get current telemetry as ResQTerra Telemetry message
    pub async fn get_telemetry(&self) -> Telemetry {
        Telemetry {
            position: self.position.read().await.clone(),
            battery: self.battery.read().await.clone(),
            state: (*self.state.read().await).into(),
            fc_status: Some(self.fc_status.read().await.clone()),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            conn_quality: Some(ConnectionQuality {
                active_transport: Transport::Transport5g.into(),
                rssi_dbm: 0,
                latency_ms: 0,
                packet_loss_percent: 0.0,
            }),
        }
    }
}

impl Default for TelemetryReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert ArduPilot mode number to string
fn mode_to_string(mode: u32) -> String {
    match mode {
        0 => "STABILIZE".to_string(),
        1 => "ACRO".to_string(),
        2 => "ALT_HOLD".to_string(),
        3 => "AUTO".to_string(),
        4 => "GUIDED".to_string(),
        5 => "LOITER".to_string(),
        6 => "RTL".to_string(),
        7 => "CIRCLE".to_string(),
        9 => "LAND".to_string(),
        11 => "DRIFT".to_string(),
        13 => "SPORT".to_string(),
        14 => "FLIP".to_string(),
        15 => "AUTOTUNE".to_string(),
        16 => "POSHOLD".to_string(),
        17 => "BRAKE".to_string(),
        18 => "THROW".to_string(),
        19 => "AVOID_ADSB".to_string(),
        20 => "GUIDED_NOGPS".to_string(),
        21 => "SMART_RTL".to_string(),
        _ => format!("UNKNOWN({})", mode),
    }
}

/// Convert MAVLink severity to string
fn severity_to_string(severity: u8) -> &'static str {
    match severity {
        0 => "EMERGENCY",
        1 => "ALERT",
        2 => "CRITICAL",
        3 => "ERROR",
        4 => "WARNING",
        5 => "NOTICE",
        6 => "INFO",
        7 => "DEBUG",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_telemetry_reader_creation() {
        let reader = TelemetryReader::new();
        assert_eq!(reader.get_state().await, DroneState::DroneIdle);
        assert!(!reader.is_armed().await);
    }

    #[test]
    fn test_mode_to_string() {
        assert_eq!(mode_to_string(0), "STABILIZE");
        assert_eq!(mode_to_string(4), "GUIDED");
        assert_eq!(mode_to_string(6), "RTL");
    }
}
