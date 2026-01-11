//! Safety Monitor
//!
//! Runs a background task that monitors safety conditions and triggers
//! appropriate responses when thresholds are exceeded.

use resqterra_shared::{
    now_ms, safety,
    state_machine::{SafetyStateMachine, TransitionResult},
    DroneState,
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{info, warn, error};

/// Actions that the safety monitor can trigger
#[derive(Debug, Clone)]
pub enum SafetyAction {
    /// Trigger Return-to-Home
    ReturnToHome { reason: String },
    /// Trigger emergency stop
    EmergencyStop { reason: String },
    /// State changed
    StateChanged { from: DroneState, to: DroneState },
}

/// The safety monitor manages the drone state machine and monitors safety conditions
pub struct SafetyMonitor {
    /// The state machine
    fsm: Arc<RwLock<SafetyStateMachine>>,
    /// Channel to send safety actions
    action_tx: mpsc::UnboundedSender<SafetyAction>,
    /// Channel to receive safety actions
    action_rx: Arc<RwLock<mpsc::UnboundedReceiver<SafetyAction>>>,
    /// Flag to track if monitoring is active
    monitoring_active: Arc<RwLock<bool>>,
}

impl SafetyMonitor {
    /// Create a new safety monitor
    pub fn new() -> Self {
        let (action_tx, action_rx) = mpsc::unbounded_channel();

        Self {
            fsm: Arc::new(RwLock::new(SafetyStateMachine::new())),
            action_tx,
            action_rx: Arc::new(RwLock::new(action_rx)),
            monitoring_active: Arc::new(RwLock::new(false)),
        }
    }

    /// Update the server heartbeat timestamp (call when receiving heartbeat from server)
    pub async fn update_server_heartbeat(&self) {
        self.fsm.write().await.update_heartbeat(now_ms());
    }

    /// Receive the next safety action (blocks until available)
    pub async fn recv_action(&self) -> Option<SafetyAction> {
        self.action_rx.write().await.recv().await
    }

    /// Start the safety monitoring background task
    /// Returns a handle that can be used to stop monitoring
    pub async fn start_monitoring(&self) -> SafetyMonitorHandle {
        let mut active = self.monitoring_active.write().await;
        if *active {
            panic!("Safety monitoring already active");
        }
        *active = true;
        drop(active);

        let fsm = self.fsm.clone();
        let action_tx = self.action_tx.clone();
        let monitoring_active = self.monitoring_active.clone();

        let handle = tokio::spawn(async move {
            let check_interval = Duration::from_millis(safety::HEARTBEAT_INTERVAL_MS);
            let mut ticker = interval(check_interval);

            loop {
                ticker.tick().await;

                // Check if we should stop
                if !*monitoring_active.read().await {
                    break;
                }

                // Check safety conditions
                let current_time = now_ms();
                let fsm_guard = fsm.read().await;

                let events = fsm_guard.check_safety(current_time);
                drop(fsm_guard);

                // Process any safety events
                for event in events {
                    let mut fsm_guard = fsm.write().await;
                    let from_state = fsm_guard.state();
                    let result = fsm_guard.process_event(event.clone());
                    drop(fsm_guard);

                    let action = match result {
                        TransitionResult::Success(to_state) if from_state != to_state => {
                            info!(
                                "[SAFETY] Auto-transition: {:?} -> {:?}",
                                from_state, to_state
                            );
                            SafetyAction::StateChanged {
                                from: from_state,
                                to: to_state,
                            }
                        }
                        TransitionResult::EmergencyRth { reason } => {
                            warn!("[SAFETY] AUTO-RTH TRIGGERED: {}", reason);
                            SafetyAction::ReturnToHome { reason }
                        }
                        TransitionResult::EmergencyStop { reason } => {
                            error!("[SAFETY] AUTO-EMERGENCY TRIGGERED: {}", reason);
                            SafetyAction::EmergencyStop { reason }
                        }
                        _ => continue,
                    };

                    let _ = action_tx.send(action);
                }
            }

            info!("[SAFETY] Monitoring stopped");
        });

        SafetyMonitorHandle {
            _task: handle,
        }
    }
}

impl Default for SafetyMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to stop safety monitoring
pub struct SafetyMonitorHandle {
    _task: tokio::task::JoinHandle<()>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_safety_monitor_creation() {
        let monitor = SafetyMonitor::new();
        // Just verify it doesn't panic
        let _ = monitor;
    }

    #[tokio::test]
    async fn test_heartbeat_update() {
        let monitor = SafetyMonitor::new();
        monitor.update_server_heartbeat().await;
    }
}