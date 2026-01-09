//! Command execution infrastructure for the edge device
//!
//! This module handles:
//! - Receiving and validating commands from server
//! - Dispatching to appropriate command handlers
//! - Generating ACK responses
//! - Tracking command execution state

mod executor;
pub mod handlers;

pub use executor::{CommandExecutor, CommandResult};
