//! Timeout tracking for pending commands

use super::dispatcher::CommandDispatcher;
use std::sync::Arc;
use tokio::time::{interval, Duration};

/// Monitors command timeouts and triggers retries
pub struct TimeoutTracker {
    dispatcher: Arc<CommandDispatcher>,
    check_interval: Duration,
}

impl TimeoutTracker {
    /// Create a new timeout tracker
    pub fn new(dispatcher: Arc<CommandDispatcher>) -> Self {
        Self {
            dispatcher,
            check_interval: Duration::from_millis(1000), // Check every second
        }
    }

    /// Start the timeout monitoring loop
    pub async fn run(&self) {
        let mut ticker = interval(self.check_interval);

        loop {
            ticker.tick().await;

            // Check for timed out commands
            let timed_out = self.dispatcher.get_timed_out_commands().await;

            for cmd in timed_out {
                if cmd.can_retry() {
                    println!(
                        "Command {} timed out, retrying ({}/{})",
                        cmd.command_id,
                        cmd.retries + 1,
                        cmd.max_retries
                    );
                    if let Err(e) = self.dispatcher.retry_command(cmd.command_id).await {
                        eprintln!("Retry failed for command {}: {}", cmd.command_id, e);
                    }
                } else {
                    println!(
                        "Command {} failed after {} retries",
                        cmd.command_id, cmd.retries
                    );
                    // Command will be cleaned up by cleanup_expired
                }
            }

            // Cleanup expired commands
            let expired = self.dispatcher.cleanup_expired().await;
            if !expired.is_empty() {
                println!("Cleaned up {} expired commands", expired.len());
            }
        }
    }
}
