//! Drone State Machine
//!
//! Defines valid state transitions and safety-critical event handling.

use crate::{DroneState, safety};

/// Events that can trigger state transitions
#[derive(Debug, Clone, PartialEq)]
pub enum SafetyEvent {
    /// System startup complete
    Initialized,
    /// Pre-flight checks passed
    PreflightComplete,
    /// Drone armed and ready
    Armed,
    /// Takeoff initiated
    TakeoffStarted,
    /// Reached mission altitude
    TakeoffComplete,
    /// Mission started
    MissionStarted,
    /// Mission completed successfully
    MissionComplete,
    /// Return-to-home initiated
    RthTriggered,
    /// Reached home position
    RthComplete,
    /// Landing initiated
    LandingStarted,
    /// Landed and disarmed
    Landed,
    /// Emergency triggered
    EmergencyTriggered,
    /// Emergency cleared
    EmergencyCleared,
    /// Heartbeat timeout (server connection lost)
    HeartbeatTimeout,
    /// Battery critical level reached
    BatteryCritical,
    /// Geofence breach
    GeofenceBreach,
    /// Command timeout
    CommandTimeout,
}

/// Result of a state transition attempt
#[derive(Debug, Clone)]
pub enum TransitionResult {
    /// Transition was valid and state changed
    Success(DroneState),
    /// Transition was invalid from current state
    Invalid { from: DroneState, event: SafetyEvent },
    /// Transition triggered emergency RTH
    EmergencyRth { reason: String },
    /// Transition triggered emergency stop
    EmergencyStop { reason: String },
}

/// The safety state machine for drone operations
#[derive(Debug)]
pub struct SafetyStateMachine {
    current_state: DroneState,
    last_server_heartbeat_ms: u64,
    battery_percent: u32,
}

impl Default for SafetyStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl SafetyStateMachine {
    /// Create a new state machine in Idle state
    pub fn new() -> Self {
        Self {
            current_state: DroneState::DroneIdle,
            last_server_heartbeat_ms: 0,
            battery_percent: 100,
        }
    }

    /// Get current state
    pub fn state(&self) -> DroneState {
        self.current_state
    }

    /// Update server heartbeat timestamp
    pub fn update_heartbeat(&mut self, timestamp_ms: u64) {
        self.last_server_heartbeat_ms = timestamp_ms;
    }

    /// Update battery level
    pub fn update_battery(&mut self, percent: u32) {
        self.battery_percent = percent;
    }

    /// Check if we've lost connection to server
    pub fn is_heartbeat_timed_out(&self, current_time_ms: u64) -> bool {
        if self.last_server_heartbeat_ms == 0 {
            return false; // Never received heartbeat yet
        }
        let elapsed = current_time_ms.saturating_sub(self.last_server_heartbeat_ms);
        elapsed > safety::HEARTBEAT_TIMEOUT_MS
    }

    /// Check if battery is at critical level
    pub fn is_battery_critical(&self) -> bool {
        self.battery_percent <= safety::BATTERY_CRITICAL_PERCENT
    }

    /// Process an event and return the transition result
    pub fn process_event(&mut self, event: SafetyEvent) -> TransitionResult {
        // Safety-critical events always take priority
        match &event {
            SafetyEvent::EmergencyTriggered => {
                let prev = self.current_state;
                self.current_state = DroneState::DroneEmergency;
                return TransitionResult::EmergencyStop {
                    reason: format!("Emergency triggered from {:?}", prev),
                };
            }
            SafetyEvent::HeartbeatTimeout => {
                return self.trigger_safety_rth("Server heartbeat timeout");
            }
            SafetyEvent::BatteryCritical => {
                return self.trigger_safety_rth("Battery critical");
            }
            SafetyEvent::GeofenceBreach => {
                return self.trigger_safety_rth("Geofence breach");
            }
            _ => {}
        }

        // Normal state transitions
        let new_state = self.get_next_state(&event);

        match new_state {
            Some(state) => {
                self.current_state = state;
                TransitionResult::Success(state)
            }
            None => TransitionResult::Invalid {
                from: self.current_state,
                event,
            },
        }
    }

    /// Get the next state for a given event, if the transition is valid
    fn get_next_state(&self, event: &SafetyEvent) -> Option<DroneState> {
        use DroneState::*;
        use SafetyEvent::*;

        match (self.current_state, event) {
            // From Idle
            (DroneIdle, Initialized) => Some(DroneIdle),
            (DroneIdle, PreflightComplete) => Some(DronePreflight),

            // From Preflight
            (DronePreflight, Armed) => Some(DroneArmed),

            // From Armed
            (DroneArmed, TakeoffStarted) => Some(DroneTakingOff),

            // From TakingOff
            (DroneTakingOff, TakeoffComplete) => Some(DroneIdle), // Ready for mission
            (DroneTakingOff, MissionStarted) => Some(DroneInMission),

            // From InMission
            (DroneInMission, MissionComplete) => Some(DroneIdle),
            (DroneInMission, RthTriggered) => Some(DroneReturningHome),

            // From ReturningHome
            (DroneReturningHome, RthComplete) => Some(DroneLanding),
            (DroneReturningHome, LandingStarted) => Some(DroneLanding),

            // From Landing
            (DroneLanding, Landed) => Some(DroneIdle),

            // From Emergency - can only be cleared explicitly
            (DroneEmergency, EmergencyCleared) => Some(DroneIdle),

            // RTH can be triggered from most active states
            (DroneArmed | DroneTakingOff, RthTriggered) => {
                Some(DroneReturningHome)
            }

            // Invalid transition
            _ => None,
        }
    }

    /// Trigger safety RTH and return result
    fn trigger_safety_rth(&mut self, reason: &str) -> TransitionResult {
        match self.current_state {
            // Already safe states - no action needed
            DroneState::DroneIdle | DroneState::DroneLanding => TransitionResult::Success(self.current_state),

            // Already returning home
            DroneState::DroneReturningHome => TransitionResult::Success(self.current_state),

            // Already in emergency
            DroneState::DroneEmergency => TransitionResult::Success(self.current_state),

            // Active flight states - trigger RTH
            DroneState::DroneArmed
            | DroneState::DroneTakingOff
            | DroneState::DroneInMission
            | DroneState::DronePreflight => {
                self.current_state = DroneState::DroneReturningHome;
                TransitionResult::EmergencyRth {
                    reason: reason.to_string(),
                }
            }

            // Unknown state - go to emergency
            DroneState::DroneUnknown => {
                self.current_state = DroneState::DroneEmergency;
                TransitionResult::EmergencyStop {
                    reason: format!("{} (unknown state)", reason),
                }
            }
        }
    }

    /// Check all safety conditions and return any triggered events
    pub fn check_safety(&self, current_time_ms: u64) -> Vec<SafetyEvent> {
        let mut events = Vec::new();

        if self.is_heartbeat_timed_out(current_time_ms) {
            events.push(SafetyEvent::HeartbeatTimeout);
        }

        if self.is_battery_critical() {
            events.push(SafetyEvent::BatteryCritical);
        }

        events
    }
}

/// Check if a transition from one state to another is generally valid
pub fn is_valid_transition(from: DroneState, to: DroneState) -> bool {
    use DroneState::*;

    match (from, to) {
        // Same state is always valid
        (a, b) if a == b => true,

        // Emergency can be reached from anywhere
        (_, DroneEmergency) => true,

        // Specific valid transitions
        (DroneIdle, DronePreflight) => true,
        (DronePreflight, DroneArmed) => true,
        (DroneArmed, DroneTakingOff) => true,
        (DroneTakingOff, DroneInMission) => true,
        (DroneTakingOff, DroneIdle) => true, // Aborted takeoff
        (DroneInMission, DroneReturningHome) => true,
        (DroneInMission, DroneIdle) => true, // Mission complete
        (DroneReturningHome, DroneLanding) => true,
        (DroneLanding, DroneIdle) => true,
        (DroneEmergency, DroneIdle) => true, // Emergency cleared

        // RTH can be triggered from flight states
        (DroneArmed | DroneTakingOff, DroneReturningHome) => true,

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let fsm = SafetyStateMachine::new();
        assert_eq!(fsm.state(), DroneState::DroneIdle);
    }

    #[test]
    fn test_normal_mission_flow() {
        let mut fsm = SafetyStateMachine::new();

        // Preflight
        let result = fsm.process_event(SafetyEvent::PreflightComplete);
        assert!(matches!(result, TransitionResult::Success(DroneState::DronePreflight)));

        // Arm
        let result = fsm.process_event(SafetyEvent::Armed);
        assert!(matches!(result, TransitionResult::Success(DroneState::DroneArmed)));

        // Takeoff
        let result = fsm.process_event(SafetyEvent::TakeoffStarted);
        assert!(matches!(result, TransitionResult::Success(DroneState::DroneTakingOff)));

        // Start mission
        let result = fsm.process_event(SafetyEvent::MissionStarted);
        assert!(matches!(result, TransitionResult::Success(DroneState::DroneInMission)));

        // RTH
        let result = fsm.process_event(SafetyEvent::RthTriggered);
        assert!(matches!(result, TransitionResult::Success(DroneState::DroneReturningHome)));

        // Landing
        let result = fsm.process_event(SafetyEvent::LandingStarted);
        assert!(matches!(result, TransitionResult::Success(DroneState::DroneLanding)));

        // Landed
        let result = fsm.process_event(SafetyEvent::Landed);
        assert!(matches!(result, TransitionResult::Success(DroneState::DroneIdle)));
    }

    #[test]
    fn test_heartbeat_timeout_triggers_rth() {
        let mut fsm = SafetyStateMachine::new();

        // Get into mission state
        fsm.process_event(SafetyEvent::PreflightComplete);
        fsm.process_event(SafetyEvent::Armed);
        fsm.process_event(SafetyEvent::TakeoffStarted);
        fsm.process_event(SafetyEvent::MissionStarted);

        assert_eq!(fsm.state(), DroneState::DroneInMission);

        // Heartbeat timeout
        let result = fsm.process_event(SafetyEvent::HeartbeatTimeout);
        assert!(matches!(result, TransitionResult::EmergencyRth { .. }));
        assert_eq!(fsm.state(), DroneState::DroneReturningHome);
    }

    #[test]
    fn test_emergency_from_any_state() {
        let mut fsm = SafetyStateMachine::new();

        // Emergency from idle
        let result = fsm.process_event(SafetyEvent::EmergencyTriggered);
        assert!(matches!(result, TransitionResult::EmergencyStop { .. }));
        assert_eq!(fsm.state(), DroneState::DroneEmergency);
    }

    #[test]
    fn test_invalid_transition() {
        let mut fsm = SafetyStateMachine::new();

        // Can't arm directly from idle (need preflight first)
        let result = fsm.process_event(SafetyEvent::Armed);
        assert!(matches!(result, TransitionResult::Invalid { .. }));
        assert_eq!(fsm.state(), DroneState::DroneIdle);
    }

    #[test]
    fn test_heartbeat_timeout_detection() {
        let mut fsm = SafetyStateMachine::new();

        // No heartbeat yet - should not timeout
        assert!(!fsm.is_heartbeat_timed_out(1000));

        // Receive heartbeat
        fsm.update_heartbeat(1000);

        // Shortly after - should not timeout
        assert!(!fsm.is_heartbeat_timed_out(2000));

        // After timeout period
        let timeout_time = 1000 + safety::HEARTBEAT_TIMEOUT_MS + 1;
        assert!(fsm.is_heartbeat_timed_out(timeout_time));
    }
}
