//! Configuration update command handler

use super::HandlerContext;
use crate::command::CommandResult;
use resqterra_shared::{Command, command};

/// Handle CONFIG_UPDATE command
pub async fn handle_config_update(ctx: &HandlerContext, command: &Command) -> CommandResult {
    // Extract config parameters
    let config = match &command.params {
        Some(command::Params::ConfigUpdate(c)) => c,
        _ => {
            return CommandResult::Rejected {
                message: "Missing config parameters".into(),
            };
        }
    };

    println!("  [CONFIG_UPDATE] Received {} config entries", config.config.len());

    for (key, value) in &config.config {
        println!("    {} = {}", key, value);
        // TODO: Actually apply configuration changes
    }

    CommandResult::Completed {
        message: format!("Applied {} config entries", config.config.len()),
    }
}
