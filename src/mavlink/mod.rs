//! MAVLink Bridge Module
//!
//! Provides integration with ArduPilot/PX4 flight controllers via MAVLink protocol.
//! Supports both serial and UDP connections.

mod commands;
mod connection;
mod telemetry;

pub use commands::{ArduPilotMode, MavCommandSender};
pub use connection::{FcConfig, FcConnectionType, FcEvent, FlightController};
pub use telemetry::TelemetryReader;
