//! Safety Module
//!
//! Monitors safety conditions and triggers automatic responses
//! such as Return-to-Home on connection loss.

mod monitor;

pub use monitor::{SafetyMonitor, SafetyAction};
