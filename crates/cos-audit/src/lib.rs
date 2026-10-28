//! `cos-audit` defines immutable append-only audit events and recording contracts.
//!
//! # Invariants:
//! 1. Historical audit events cannot be edited or deleted.
//! 2. Every security boundary crossing, approval, grant decision, and tool action emits an audit record.

use chrono::{DateTime, Utc};
use cos_contracts::{ActionDigest, TraceContext};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Types of domain and security events captured in the immutable audit trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditEventType {
    CommandReceived,
    GrantIssued,
    GrantDenied,
    ApprovalRequested,
    ApprovalDecided,
    ToolExecutionStarted,
    ToolExecutionCompleted,
    ToolExecutionDenied,
    StateTransitioned,
}

/// An immutable, append-only record of an action, decision, or state transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub audit_id: Uuid,
    pub context: TraceContext,
    pub event_type: AuditEventType,
    pub action_digest: Option<ActionDigest>,
    pub details: String,
    pub recorded_at: DateTime<Utc>,
}

impl AuditEvent {
    pub fn new(
        context: TraceContext,
        event_type: AuditEventType,
        action_digest: Option<ActionDigest>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            audit_id: Uuid::now_v7(),
            context,
            event_type,
            action_digest,
            details: details.into(),
            recorded_at: Utc::now(),
        }
    }
}

/// Port defining append-only audit writing capabilities.
pub trait AuditWriterPort: Send + Sync {
    fn record(&self, event: AuditEvent) -> Result<(), String>;
}

/// In-memory thread-safe implementation of AuditWriterPort for unit and integration testing.
#[derive(Debug, Default)]
pub struct InMemoryAuditLogger {
    events: std::sync::RwLock<Vec<AuditEvent>>,
}

impl InMemoryAuditLogger {
    pub fn new() -> Self {
        Self {
            events: std::sync::RwLock::new(Vec::new()),
        }
    }

    pub fn events(&self) -> Vec<AuditEvent> {
        self.events.read().unwrap().clone()
    }

    pub fn len(&self) -> usize {
        self.events.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.read().unwrap().is_empty()
    }
}

impl AuditWriterPort for InMemoryAuditLogger {
    fn record(&self, event: AuditEvent) -> Result<(), String> {
        self.events.write().unwrap().push(event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cos_identity::ActorIdentity;

    #[test]
    fn test_audit_event_creation_and_recording() {
        let logger = InMemoryAuditLogger::new();
        let context = TraceContext::root(ActorIdentity::system("test_service"));

        let event = AuditEvent::new(
            context,
            AuditEventType::GrantIssued,
            Some(ActionDigest::compute(b"action_1")),
            "Task grant issued successfully",
        );

        logger
            .record(event.clone())
            .expect("Failed to record audit event");
        assert_eq!(logger.len(), 1);

        let events = logger.events();
        assert_eq!(events[0].event_type, AuditEventType::GrantIssued);
        assert_eq!(events[0].details, "Task grant issued successfully");
    }
}
