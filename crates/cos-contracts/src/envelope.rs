use chrono::{DateTime, Utc};
use cos_domain::{ApprovalId, ClientScope, GoalId, ProjectScope, TaskId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::digest::ActionDigest;
use crate::grant::TaskGrant;
use crate::trace::TraceContext;

/// Command envelope representing an explicit intent to execute an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEnvelope<T> {
    pub command_id: Uuid,
    pub command_type: String,
    pub context: TraceContext,
    pub task_id: Option<TaskId>,
    pub goal_id: Option<GoalId>,
    pub client_scope: Option<ClientScope>,
    pub project_scope: Option<ProjectScope>,
    pub payload: T,
    pub expected_version: u64,
    pub capability_grant: Option<TaskGrant>,
    pub approval_reference: Option<ApprovalId>,
    pub action_digest: ActionDigest,
    pub idempotency_key: String,
    pub created_at: DateTime<Utc>,
}

/// Event envelope representing a completed, immutable state transition fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub aggregate_version: u64,
    pub context: TraceContext,
    pub source: String,
    pub payload: T,
    pub occurred_at: DateTime<Utc>,
}
