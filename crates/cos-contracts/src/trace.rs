use chrono::{DateTime, Utc};
use cos_domain::{CausationId, CorrelationId, TraceId};
use cos_identity::ActorIdentity;
use serde::{Deserialize, Serialize};

/// Canonical distributed tracing context required on every command, event, approval, and audit record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<CausationId>,
    pub actor: ActorIdentity,
    pub recorded_at: DateTime<Utc>,
    pub schema_version: u32,
}

impl TraceContext {
    /// Creates a new root tracing context for an initial intake or root operation.
    pub fn root(actor: ActorIdentity) -> Self {
        Self {
            trace_id: TraceId::new_v7(),
            correlation_id: CorrelationId::new_v7(),
            causation_id: None,
            actor,
            recorded_at: Utc::now(),
            schema_version: 1,
        }
    }

    /// Derives a child trace context linking to this parent operation.
    pub fn derive_child(&self, actor: ActorIdentity, parent_operation_id: CausationId) -> Self {
        Self {
            trace_id: self.trace_id,
            correlation_id: self.correlation_id,
            causation_id: Some(parent_operation_id),
            actor,
            recorded_at: Utc::now(),
            schema_version: self.schema_version,
        }
    }
}
