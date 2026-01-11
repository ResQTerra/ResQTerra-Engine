//! Connection management for persistent bidirectional communication
//!
//! This module handles:
//! - Persistent TCP connections with automatic reconnection
//! - Transport failover (5G primary, Bluetooth fallback)
//! - Bidirectional message streaming
//! - Heartbeat management

mod manager;

pub use manager::{ConnectionConfig, ConnectionEvent, ConnectionManager};
