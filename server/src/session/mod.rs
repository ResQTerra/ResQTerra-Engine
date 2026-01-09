//! Session management for tracking connected drones
//!
//! This module handles:
//! - Tracking all connected drone sessions
//! - Bidirectional message routing
//! - Heartbeat monitoring and dead drone detection
//! - Command dispatch to specific drones

mod manager;
mod connection;

pub use manager::SessionManager;
pub use connection::{DroneSession, SessionHandle};
