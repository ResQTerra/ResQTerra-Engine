//! Command handlers for different command types

mod mission;
mod rth;
mod status;
mod config;
mod emergency;

pub use mission::{handle_mission_start, handle_mission_abort};
pub use rth::handle_rth;
pub use status::handle_status_request;
pub use config::handle_config_update;
pub use emergency::handle_emergency_stop;

use resqterra_shared::DroneState;

/// Context passed to command handlers
#[derive(Debug, Clone)]
pub struct HandlerContext {
    pub device_id: String,
    pub current_state: DroneState,
    pub command_id: u64,
}
