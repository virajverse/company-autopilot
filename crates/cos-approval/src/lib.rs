//! `cos-approval` manages the approval request lifecycle, exact action-digest binding,
//! and human decision recording.
//!
//! # Laws Enforced:
//! 1. Approval is explicit: timeout or high agent confidence is never approval.
//! 2. R4 Critical actions strictly require explicit Founder authorization.
//! 3. Approvals are strictly one-time consumable to prevent replay attacks.

use chrono::{DateTime, Utc};
use cos_contracts::{ActionDigest, TraceContext};
use cos_domain::{ApprovalId, ApprovalStatus, ClientScope, RiskClass, TaskId};
use cos_identity::ActorIdentity;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ApprovalError {
    #[error("Approval request has expired")]
    Expired,

    #[error("Invalid state transition from '{from:?}' to '{to:?}'")]
    InvalidTransition {
        from: ApprovalStatus,
        to: ApprovalStatus,
    },

    #[error("Unauthorized approver: R4 actions strictly require Founder identity")]
    FounderRequired,

    #[error("Approval already consumed")]
    AlreadyConsumed,
}

/// Abstract read-only port to query approval states across decoupled layers.
pub trait ApprovalQueryPort: Send + Sync {
    fn get_approval(&self, id: &ApprovalId) -> Option<ApprovalRequest>;
}

/// An approval request bound to an exact ActionDigest and Task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub approval_id: ApprovalId,
    pub task_id: TaskId,
    pub context: TraceContext,
    pub proposed_action: String,
    pub action_digest: ActionDigest,
    pub risk_class: RiskClass,
    pub client_scope: Option<ClientScope>,
    pub status: ApprovalStatus,
    pub decision_actor: Option<ActorIdentity>,
    pub decision_reason: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
}

impl ApprovalRequest {
    pub fn new(
        task_id: TaskId,
        context: TraceContext,
        proposed_action: impl Into<String>,
        action_digest: ActionDigest,
        risk_class: RiskClass,
        client_scope: Option<ClientScope>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            approval_id: ApprovalId::new_v7(),
            task_id,
            context,
            proposed_action: proposed_action.into(),
            action_digest,
            risk_class,
            client_scope,
            status: ApprovalStatus::Submitted,
            decision_actor: None,
            decision_reason: None,
            decided_at: None,
            expires_at,
        }
    }

    /// Records an approval decision by an authorized actor.
    pub fn approve(
        &mut self,
        approver: ActorIdentity,
        reason: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), ApprovalError> {
        if now > self.expires_at {
            self.status = ApprovalStatus::Expired;
            return Err(ApprovalError::Expired);
        }

        if self.status != ApprovalStatus::Submitted
            && self.status != ApprovalStatus::NeedsClarification
        {
            return Err(ApprovalError::InvalidTransition {
                from: self.status,
                to: ApprovalStatus::Approved,
            });
        }

        // R4 invariant: Approver MUST be Founder
        if self.risk_class.is_founder_only() && !approver.actor_category.is_founder() {
            return Err(ApprovalError::FounderRequired);
        }

        self.status = ApprovalStatus::Approved;
        self.decision_actor = Some(approver);
        self.decision_reason = Some(reason.into());
        self.decided_at = Some(now);

        Ok(())
    }

    /// Records a rejection decision by an authorized actor.
    pub fn reject(
        &mut self,
        approver: ActorIdentity,
        reason: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), ApprovalError> {
        if self.status != ApprovalStatus::Submitted
            && self.status != ApprovalStatus::NeedsClarification
        {
            return Err(ApprovalError::InvalidTransition {
                from: self.status,
                to: ApprovalStatus::Rejected,
            });
        }

        self.status = ApprovalStatus::Rejected;
        self.decision_actor = Some(approver);
        self.decision_reason = Some(reason.into());
        self.decided_at = Some(now);

        Ok(())
    }

    /// Consumes the approval for execution. Can only be done once.
    pub fn consume(&mut self) -> Result<(), ApprovalError> {
        if self.status == ApprovalStatus::Consumed {
            return Err(ApprovalError::AlreadyConsumed);
        }

        if self.status != ApprovalStatus::Approved {
            return Err(ApprovalError::InvalidTransition {
                from: self.status,
                to: ApprovalStatus::Consumed,
            });
        }

        self.status = ApprovalStatus::Consumed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use cos_domain::SubjectId;

    #[test]
    fn test_approval_lifecycle() {
        let now = Utc::now();
        let digest = ActionDigest::compute(b"quote_5000");
        let context = TraceContext::root(ActorIdentity::system("executive"));

        let mut req = ApprovalRequest::new(
            TaskId::new_v7(),
            context,
            "Send quotation of $5000",
            digest,
            RiskClass::R3BusinessCommitment,
            None,
            now + Duration::minutes(15),
        );

        assert_eq!(req.status, ApprovalStatus::Submitted);

        // Approve with human approver
        let approver = ActorIdentity::founder(SubjectId::new_v7(), None);
        assert!(req.approve(approver.clone(), "Approved price", now).is_ok());
        assert_eq!(req.status, ApprovalStatus::Approved);

        // Consume approval
        assert!(req.consume().is_ok());
        assert_eq!(req.status, ApprovalStatus::Consumed);

        // Consuming again fails (replay protection)
        assert_eq!(req.consume(), Err(ApprovalError::AlreadyConsumed));
    }

    #[test]
    fn test_r4_rejects_non_founder_approval() {
        let now = Utc::now();
        let digest = ActionDigest::compute(b"refund_transaction");
        let context = TraceContext::root(ActorIdentity::system("executive"));

        let mut req = ApprovalRequest::new(
            TaskId::new_v7(),
            context,
            "Process refund of $1000",
            digest,
            RiskClass::R4IrreversibleCritical,
            None,
            now + Duration::minutes(15),
        );

        // Agent tries to approve R4 -> rejected with FounderRequired
        let agent = ActorIdentity::agent(SubjectId::new_v7(), "support");
        assert_eq!(
            req.approve(agent, "Self-approved", now),
            Err(ApprovalError::FounderRequired)
        );

        // Founder approves R4 -> succeeds
        let founder = ActorIdentity::founder(SubjectId::new_v7(), None);
        assert!(req
            .approve(founder, "Founder authorized refund", now)
            .is_ok());
    }
}
