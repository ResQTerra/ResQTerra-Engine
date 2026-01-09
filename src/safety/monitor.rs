//! Safety Monitor
//!
//! Runs a background task that monitors safety conditions and triggers
//! appropriate responses when thresholds are exceeded.

use resqterra_shared::{
    now_ms, safety,
    state_machine::{SafetyEvent, SafetyStateMachine, TransitionResult},
    DroneState,
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};

/// Actions that the safety monitor can trigger
#[derive(Debug, Clone)]
pub enum SafetyAction {
    /// Trigger Return-to-Home
    ReturnToHome { reason: String },
    /// Trigger emergency stop
    EmergencyStop { reason: String },
    /// State changed
    StateChanged { from: DroneState, to: DroneState },
    /// No action needed
    None,
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

    /// Get the current drone state
    pub async fn state(&self) -> DroneState {
        self.fsm.read().await.state()
    }

    /// Update the server heartbeat timestamp (call when receiving heartbeat from server)
    pub async fn update_server_heartbeat(&self) {
        self.fsm.write().await.update_heartbeat(now_ms());
    }

    /// Update battery level
    pub async fn update_battery(&self, percent: u32) {
        let mut fsm = self.fsm.write().await;
        fsm.update_battery(percent);

        // Check if this triggers a safety event
        if fsm.is_battery_critical() {
            drop(fsm); // Release lock before processing
            let _ = self.process_event(SafetyEvent::BatteryCritical).await;
        }
    }

    /// Process a safety event and return the resulting action
    pub async fn process_event(&self, event: SafetyEvent) -> SafetyAction {
        let mut fsm = self.fsm.write().await;
        let from_state = fsm.state();
        let result = fsm.process_event(event);

        let action = match result {
            TransitionResult::Success(to_state) => {
                if from_state != to_state {
                    println!(
                        "[SAFETY] State transition: {:?} -> {:?}",
                        from_state, to_state
                    );
                    SafetyAction::StateChanged {
                        from: from_state,
                        to: to_state,
                    }
                } else {
                    SafetyAction::None
                }
            }
            TransitionResult::Invalid { from, event } => {
                eprintln!(
                    "[SAFETY] Invalid transition: {:?} from state {:?}",
                    event, from
                );
                SafetyAction::None
            }
            TransitionResult::EmergencyRth { reason } => {
                println!("[SAFETY] EMERGENCY RTH: {}", reason);
                SafetyAction::ReturnToHome { reason }
            }
            TransitionResult::EmergencyStop { reason } => {
                println!("[SAFETY] EMERGENCY STOP: {}", reason);
                SafetyAction::EmergencyStop { reason }
            }
        };

        // Send action to channel for external handlers
        if !matches!(action, SafetyAction::None) {
            let _ = self.action_tx.send(action.clone());
        }

        action
    }

    /// Trigger Return-to-Home manually
    pub async fn trigger_rth(&self) -> SafetyAction {
        self.process_event(SafetyEvent::RthTriggered).await
    }

    /// Trigger emergency stop
    pub async fn trigger_emergency(&self) -> SafetyAction {
        self.process_event(SafetyEvent::EmergencyTriggered).await
    }

    /// Start mission
    pub async fn start_mission(&self) -> SafetyAction {
        self.process_event(SafetyEvent::MissionStarted).await
    }

    /// Complete mission
    pub async fn complete_mission(&self) -> SafetyAction {
        self.process_event(SafetyEvent::MissionComplete).await
    }

    /// Receive the next safety action (blocks until available)
    pub async fn recv_action(&self) -> Option<SafetyAction> {
        self.action_rx.write().await.recv().await
    }

    /// Try to receive a safety action without blocking
    pub async fn try_recv_action(&self) -> Option<SafetyAction> {
        self.action_rx.write().await.try_recv().ok()
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
                let mut fsm_guard = fsm.write().await;

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
                            println!(
                                "[SAFETY] Auto-transition: {:?} -> {:?}",
                                from_state, to_state
                            );
                            SafetyAction::StateChanged {
                                from: from_state,
                                to: to_state,
                            }
                        }
                        TransitionResult::EmergencyRth { reason } => {
                            println!("[SAFETY] AUTO-RTH TRIGGERED: {}", reason);
                            SafetyAction::ReturnToHome { reason }
                        }
                        TransitionResult::EmergencyStop { reason } => {
                            println!("[SAFETY] AUTO-EMERGENCY TRIGGERED: {}", reason);
                            SafetyAction::EmergencyStop { reason }
                        }
                        _ => continue,
                    };

                    let _ = action_tx.send(action);
                }
            }

            println!("[SAFETY] Monitoring stopped");
        });

        SafetyMonitorHandle {
            monitoring_active: self.monitoring_active.clone(),
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
    monitoring_active: Arc<RwLock<bool>>,
    _task: tokio::task::JoinHandle<()>,
}

impl SafetyMonitorHandle {
    /// Stop the safety monitoring
    pub async fn stop(&self) {
        *self.monitoring_active.write().await = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_safety_monitor_creation() {
        let monitor = SafetyMonitor::new();
        assert_eq!(monitor.state().await, DroneState::DroneIdle);
    }

    #[tokio::test]
    async fn test_heartbeat_update() {
        let monitor = SafetyMonitor::new();
        monitor.update_server_heartbeat().await;
        // Should not trigger any action immediately
        assert!(monitor.try_recv_action().await.is_none());
    }

    #[tokio::test]
    async fn test_manual_rth() {
        let monitor = SafetyMonitor::new();

        // Need to be in a flying state first
        monitor.process_event(SafetyEvent::PreflightComplete).await;
        monitor.process_event(SafetyEvent::Armed).await;
        monitor.process_event(SafetyEvent::TakeoffStarted).await;
        monitor.process_event(SafetyEvent::MissionStarted).await;

        let action = monitor.trigger_rth().await;
        assert!(matches!(action, SafetyAction::StateChanged { to: DroneState::DroneReturningHome, .. }));
    }

    #[tokio::test]
    async fn test_emergency_stop() {
        let monitor = SafetyMonitor::new();

        let action = monitor.trigger_emergency().await;
        assert!(matches!(action, SafetyAction::EmergencyStop { .. }));
        assert_eq!(monitor.state().await, DroneState::DroneEmergency);
    }
}
