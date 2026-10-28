use chrono::{DateTime, Utc};
use cos_domain::{
    AgentId, ApprovalId, ClientScope, GrantId, GrantStatus, ProjectScope, RiskClass, SubjectId,
    TaskId, TenantId, TraceId,
};
use cos_identity::ActorIdentity;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::digest::ActionDigest;

/// Specific atomic capability that an agent or tool can be granted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Capability(pub String);

impl Capability {
    pub fn new(cap: impl Into<String>) -> Self {
        Self(cap.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Canonical TaskGrant issued by Governance.
///
/// Invariant:
/// - Side-effecting grants (R1-R4) are bound to exactly ONE action_digest.
/// - Read-only grants (R0) are bound to a read-scope action_digest.
/// - Grants are short-lived and expire at `expires_at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGrant {
    pub grant_id: GrantId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub subject_id: SubjectId,
    pub role: String,
    pub allowed_capabilities: Vec<Capability>,
    pub client_scope: Option<ClientScope>,
    pub project_scope: Option<ProjectScope>,
    pub tenant_scope: TenantId,
    pub risk_class: RiskClass,
    pub approval_reference: Option<ApprovalId>,
    pub action_digest: ActionDigest,
    pub policy_version: u32,
    pub grant_status: GrantStatus,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub max_calls: u32,
    pub issued_by: ActorIdentity,
    pub audit_trace_id: TraceId,
    pub revocation_reference: Option<String>,
}

impl TaskGrant {
    /// Validates whether the grant is currently active and usable at the given timestamp.
    pub fn is_valid_at(&self, now: DateTime<Utc>) -> bool {
        self.grant_status == GrantStatus::Active && now <= self.expires_at && self.max_calls > 0
    }

    /// Verifies if a given action digest matches the one bound to this grant.
    pub fn matches_digest(&self, digest: &ActionDigest) -> bool {
        &self.action_digest == digest
    }

    /// Verifies if the grant contains the requested capability.
    pub fn has_capability(&self, capability: &Capability) -> bool {
        self.allowed_capabilities.contains(capability)
    }
}
