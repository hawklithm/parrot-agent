use serde::{Deserialize, Serialize};
use std::fmt;

use crate::agent::AgentStatus;

/// State machine for agent status transitions
#[derive(Debug, Clone)]
pub struct AgentStateMachine {
    current_status: AgentStatus,
}

/// Transition error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionError {
    pub from: AgentStatus,
    pub to: AgentStatus,
    pub reason: String,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid transition from {:?} to {:?}: {}",
            self.from, self.to, self.reason
        )
    }
}

impl std::error::Error for TransitionError {}

impl AgentStateMachine {
    /// Create a new state machine with initial status
    pub fn new(initial_status: AgentStatus) -> Self {
        Self {
            current_status: initial_status,
        }
    }

    /// Get current status
    pub fn current(&self) -> AgentStatus {
        self.current_status
    }

    /// Check if transition is valid
    pub fn can_transition_to(&self, target: AgentStatus) -> bool {
        match (self.current_status, target) {
            // From Idle
            (AgentStatus::Idle, AgentStatus::Running) => true,
            (AgentStatus::Idle, AgentStatus::PendingApproval) => true,
            (AgentStatus::Idle, AgentStatus::Terminated) => true,

            // From Running
            (AgentStatus::Running, AgentStatus::Idle) => true,
            (AgentStatus::Running, AgentStatus::Paused) => true,
            (AgentStatus::Running, AgentStatus::Terminated) => true,

            // From Paused
            (AgentStatus::Paused, AgentStatus::Running) => true,
            (AgentStatus::Paused, AgentStatus::Idle) => true,
            (AgentStatus::Paused, AgentStatus::Terminated) => true,

            // From PendingApproval
            (AgentStatus::PendingApproval, AgentStatus::Idle) => true,
            (AgentStatus::PendingApproval, AgentStatus::Running) => true,
            (AgentStatus::PendingApproval, AgentStatus::Terminated) => true,

            // From Terminated - no transitions allowed
            (AgentStatus::Terminated, _) => false,

            // Same state is always allowed
            (a, b) if a == b => true,

            // All other transitions are invalid
            _ => false,
        }
    }

    /// Attempt to transition to a new status
    pub fn transition_to(&mut self, target: AgentStatus) -> Result<AgentStatus, TransitionError> {
        if !self.can_transition_to(target) {
            return Err(TransitionError {
                from: self.current_status,
                to: target,
                reason: format!(
                    "Transition from {:?} to {:?} is not allowed",
                    self.current_status, target
                ),
            });
        }

        let old_status = self.current_status;
        self.current_status = target;
        Ok(old_status)
    }

    /// Get all valid next states from current status
    pub fn valid_next_states(&self) -> Vec<AgentStatus> {
        match self.current_status {
            AgentStatus::Idle => vec![
                AgentStatus::Running,
                AgentStatus::PendingApproval,
                AgentStatus::Terminated,
            ],
            AgentStatus::Running => vec![
                AgentStatus::Idle,
                AgentStatus::               AgentStatus::Terminated,
            ],
            AgentStatus::Paused => vec![
                AgentStatus::Running,
                AgentStatus::Idle,
                AgentStatus::Terminated,
            ],
            AgentStatus::PendingApproval => vec![
                AgentStatus::Idle,
                AgentStatus::Running,
                AgentStatus::Terminated,
            ],
            AgentStatus::Terminated => vec![],
        }
    }

    /// Check if agent is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self.current_status, AgentStatus::Terminated)
    }

    /// Check if agent can perform work
    pub fn can_work(&self) -> bool {
        matches!(self.current_status, AgentStatus::Running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idle_to_running() {
        let mut sm = AgentStateMachine::new(AgentStatus::Idle);
        assert!(sm.can_transition_to(AgentStatus::Running));
        assert!(sm.transition_to(AgentStatus::Running).is_ok());
        assert_eq!(sm.current(), AgentStatus::Running);
    }

    #[test]
    fn test_terminated_no_transitions() {
        let mut sm = AgentStateMachine::new(AgentStatus::Terminated);
        assert!(!sm.can_transition_to(AgentStatus::Idle));
        assert!(!sm.can_transition_to(AgentStatus::Running));
        assert!(sm.transition_to(AgentStatus::Running).is_err());
    }

    #[test]
    fn test_valid_next_states() {
        let sm = AgentStateMachine::new(AgentStatus::Running);
        let next = sm.valid_next_states();
        assert!(next.contains(&AgentStatus::Idle));
        assert!(next.contains(&AgentStatus::Paused));
        assert!(next.contains(&AgentStatus::Terminated));
    }

    #[test]
    fn test_can_work() {
        let sm_running = AgentStateMachine::new(AgentStatus::Running);
        assert!(sm_running.can_work());

        let sm_idle = AgentStateMachine::new(AgentStatus::Idle);
        assert!(!sm_idle.can_work());
    }
}
