use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::DomainError;

/// Task execution state machine.
///
/// Disallowed transitions trigger DomainError.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Pending,
    Assigned,
    Planned,
    AwaitingApproval,
    Executable,
    Running,
    Completed,
    Recovery,
    Escalated,
    Rejected,
    Cancelled,
}

impl TaskStatus {
    /// Validates whether transitioning from `self` to `next` is allowed by domain rules.
    pub fn can_transition_to(&self, next: &TaskStatus) -> bool {
        match (self, next) {
            // Pending can move to Assigned or Cancelled
            (Self::Pending, Self::Assigned | Self::Cancelled) => true,

            // Assigned can move to Planned, Cancelled, or Escalated
            (Self::Assigned, Self::Planned | Self::Cancelled | Self::Escalated) => true,

            // Planned can move to AwaitingApproval (high risk) or Executable (low risk + grant valid) or Cancelled
            (Self::Planned, Self::AwaitingApproval | Self::Executable | Self::Cancelled) => true,

            // AwaitingApproval can move to Executable (approved), Rejected, or Cancelled
            (Self::AwaitingApproval, Self::Executable | Self::Rejected | Self::Cancelled) => true,

            // Executable moves to Running or Cancelled
            (Self::Executable, Self::Running | Self::Cancelled) => true,

            // Running can move to Completed, Recovery (on failure), or Escalated
            (Self::Running, Self::Completed | Self::Recovery | Self::Escalated) => true,

            // Recovery can retry back to Running, or escalate when bounded recovery is exhausted
            (Self::Recovery, Self::Running | Self::Escalated) => true,

            // Escalated can complete after human intervention or be cancelled
            (Self::Escalated, Self::Completed | Self::Cancelled | Self::Running) => true,

            // Terminal states cannot transition further
            (Self::Completed | Self::Rejected | Self::Cancelled, _) => false,

            _ => false,
        }
    }

    /// Mutates the state to `next` if allowed, otherwise returns `DomainError::InvalidStateTransition`.
    pub fn transition_to(&mut self, next: TaskStatus) -> Result<(), DomainError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(DomainError::InvalidStateTransition {
                from: format!("{:?}", self),
                to: format!("{:?}", next),
                reason: "Disallowed by Task state machine invariants".to_string(),
            })
        }
    }

    /// Returns true if this is a terminal state.
    #[inline]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Rejected | Self::Cancelled)
    }
}

/// Epistemic lifecycle status of a knowledge / memory item.
///
/// Law: UNKNOWN is an explicit missing retrieval result, not a stored lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeLifecycle {
    /// Newly observed or hypothesized candidate statement.
    Candidate,
    /// Directly observed from tool, client, or external system.
    Observed,
    /// Logically derived or synthesized from verified observations.
    Inferred,
    /// Verified by evidence, evaluation, or authoritative source.
    Verified,
    /// Deeply corroborated, high-confidence permanent operational knowledge.
    Trusted,
    /// Outdated or superseded by time or changing conditions; requires revalidation.
    Stale,
    /// Contradicted by newer evidence; requires active investigation.
    Contradicted,
    /// Under active investigative review due to contradiction.
    Investigating,
    /// Explicitly rejected claim or disproven hypothesis.
    Rejected,
}

impl KnowledgeLifecycle {
    /// Validates whether transitioning from `self` to `next` is allowed.
    pub fn can_transition_to(&self, next: &KnowledgeLifecycle) -> bool {
        match (self, next) {
            (Self::Candidate, Self::Observed | Self::Inferred | Self::Rejected) => true,
            (Self::Observed, Self::Inferred | Self::Verified | Self::Rejected) => true,
            (Self::Inferred, Self::Verified | Self::Contradicted | Self::Rejected) => true,
            (Self::Verified, Self::Trusted | Self::Stale | Self::Contradicted) => true,
            (Self::Trusted, Self::Stale | Self::Contradicted) => true,
            (Self::Stale, Self::Trusted | Self::Contradicted | Self::Rejected) => true,
            (Self::Contradicted, Self::Investigating | Self::Rejected) => true,
            (Self::Investigating, Self::Verified | Self::Rejected) => true,
            (Self::Rejected, _) => false,
            _ => false,
        }
    }

    /// Mutates the knowledge status if transition is valid.
    pub fn transition_to(&mut self, next: KnowledgeLifecycle) -> Result<(), DomainError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(DomainError::InvalidStateTransition {
                from: format!("{:?}", self),
                to: format!("{:?}", next),
                reason: "Disallowed by Knowledge lifecycle invariants".to_string(),
            })
        }
    }

    /// Returns true if this knowledge is verified or trusted and safe for operational reasoning.
    #[inline]
    pub fn is_trusted_or_verified(&self) -> bool {
        matches!(self, Self::Verified | Self::Trusted)
    }
}

/// Lifecycle status for capability grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GrantStatus {
    Issued,
    Active,
    Exhausted,
    Expired,
    Revoked,
    Denied,
}

impl GrantStatus {
    /// Returns true if the grant is currently usable for tool calls.
    #[inline]
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Lifecycle status for human and policy approvals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalStatus {
    Draft,
    Submitted,
    NeedsClarification,
    Approved,
    Rejected,
    Expired,
    Cancelled,
    Revoked,
    Consumed,
}

impl ApprovalStatus {
    /// Returns true if this approval is active and can be consumed by the Tool Gateway.
    #[inline]
    pub fn is_consumable(&self) -> bool {
        matches!(self, Self::Approved)
    }
}

/// Evidence-based autonomy state machine (Phase 10).
///
/// Autonomy expands based on verified task evidence, calibration, and stable outcomes.
/// Regressions or serious failures trigger automatic de-escalation to Supervised or Suspended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutonomyLevel {
    /// Initial mode: Agent proposals are drafts only; zero autonomous side effects.
    #[default]
    DraftOnly,
    /// Verified evidence allows supervised execution under human review.
    Supervised,
    /// Repeated success and calibration allow low-risk routine automation.
    ControlledRoutine,
    /// Broadened routine actions allowed across stable outcome domains.
    ExpandedRoutine,
    /// Circuit-breaker tripped by high-severity failure or security boundary violation.
    Suspended,
}

impl AutonomyLevel {
    /// Checks whether transitioning from `self` to `next` is permitted by governance invariants.
    pub fn can_transition_to(&self, next: &AutonomyLevel) -> bool {
        match (self, next) {
            // Identity transition
            (s, n) if s == n => true,

            // DraftOnly can advance to Supervised with verified evidence
            (Self::DraftOnly, Self::Supervised) => true,

            // Supervised can advance to ControlledRoutine with repeated success, or be Suspended on serious failure
            (Self::Supervised, Self::ControlledRoutine | Self::Suspended | Self::DraftOnly) => true,

            // ControlledRoutine can advance to ExpandedRoutine, regress to Supervised, or be Suspended
            (
                Self::ControlledRoutine,
                Self::ExpandedRoutine | Self::Supervised | Self::Suspended,
            ) => true,

            // ExpandedRoutine can regress to Supervised, or be Suspended
            (Self::ExpandedRoutine, Self::Supervised | Self::Suspended) => true,

            // Suspended can only return to DraftOnly after human review and requalification
            (Self::Suspended, Self::DraftOnly) => true,

            _ => false,
        }
    }

    /// Mutates the autonomy level if the transition is allowed.
    pub fn transition_to(&mut self, next: AutonomyLevel) -> Result<(), DomainError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(DomainError::InvalidStateTransition {
                from: format!("{:?}", self),
                to: format!("{:?}", next),
                reason: "Disallowed by Evidence-Based Autonomy lifecycle".to_string(),
            })
        }
    }

    /// Returns true if routine autonomous execution is permitted for pre-approved low-risk scopes.
    #[inline]
    pub fn permits_routine_execution(&self) -> bool {
        matches!(self, Self::ControlledRoutine | Self::ExpandedRoutine)
    }

    /// Returns true if the system or role is in safe lock-down.
    #[inline]
    pub fn is_suspended(&self) -> bool {
        matches!(self, Self::Suspended)
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for KnowledgeLifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_valid_lifecycle() {
        let mut status = TaskStatus::Pending;
        assert!(status.transition_to(TaskStatus::Assigned).is_ok());
        assert!(status.transition_to(TaskStatus::Planned).is_ok());
        assert!(status.transition_to(TaskStatus::AwaitingApproval).is_ok());
        assert!(status.transition_to(TaskStatus::Executable).is_ok());
        assert!(status.transition_to(TaskStatus::Running).is_ok());
        assert!(status.transition_to(TaskStatus::Completed).is_ok());
        assert!(status.is_terminal());

        // Completed cannot transition to Running
        assert!(status.transition_to(TaskStatus::Running).is_err());
    }

    #[test]
    fn test_task_status_forbidden_skip() {
        let mut status = TaskStatus::Pending;
        // Cannot jump directly from Pending to Running
        assert!(status.transition_to(TaskStatus::Running).is_err());
    }

    #[test]
    fn test_knowledge_lifecycle() {
        let mut k = KnowledgeLifecycle::Candidate;
        assert!(k.transition_to(KnowledgeLifecycle::Observed).is_ok());
        assert!(k.transition_to(KnowledgeLifecycle::Verified).is_ok());
        assert!(k.transition_to(KnowledgeLifecycle::Trusted).is_ok());
        assert!(k.is_trusted_or_verified());

        assert!(k.transition_to(KnowledgeLifecycle::Contradicted).is_ok());
        assert!(!k.is_trusted_or_verified());
        assert!(k.transition_to(KnowledgeLifecycle::Investigating).is_ok());
        assert!(k.transition_to(KnowledgeLifecycle::Verified).is_ok());
    }

    #[test]
    fn test_autonomy_level_lifecycle() {
        let mut level = AutonomyLevel::DraftOnly;
        assert_eq!(level, AutonomyLevel::DraftOnly);
        assert!(!level.permits_routine_execution());

        // Advance to Supervised
        assert!(level.transition_to(AutonomyLevel::Supervised).is_ok());
        assert!(!level.permits_routine_execution());

        // Advance to ControlledRoutine
        assert!(level
            .transition_to(AutonomyLevel::ControlledRoutine)
            .is_ok());
        assert!(level.permits_routine_execution());

        // Advance to ExpandedRoutine
        assert!(level.transition_to(AutonomyLevel::ExpandedRoutine).is_ok());
        assert!(level.permits_routine_execution());

        // Regress on error/drift back to Supervised
        assert!(level.transition_to(AutonomyLevel::Supervised).is_ok());
        assert!(!level.permits_routine_execution());

        // Circuit breaker: Suspend
        assert!(level.transition_to(AutonomyLevel::Suspended).is_ok());
        assert!(level.is_suspended());

        // Cannot skip directly from Suspended to ExpandedRoutine
        assert!(level.transition_to(AutonomyLevel::ExpandedRoutine).is_err());

        // Suspended requires review back to DraftOnly
        assert!(level.transition_to(AutonomyLevel::DraftOnly).is_ok());
    }
}
