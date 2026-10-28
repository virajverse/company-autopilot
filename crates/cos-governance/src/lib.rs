//! `cos-governance` enforces immutable constitutional invariants, scope boundaries,
//! risk-based approvals, and capability grant validations.
//!
//! # Core Invariants Enforced:
//! 1. Client Data Isolation: Client A's data can never be accessed under Client B's grant.
//! 2. Exact Action Binding: Any alteration to payload/target digest immediately invalidates the grant.
//! 3. R3 Commitments require verified human approval.
//! 4. R4 Critical actions require explicit founder authorization.

use chrono::{DateTime, Utc};
use cos_contracts::{ActionDigest, Capability, TaskGrant};
use cos_domain::{ClientScope, RiskClass};
use cos_identity::ActorIdentity;
use serde::{Deserialize, Serialize};

/// Result of evaluating an action request against governance and policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizationDecision {
    /// Action is fully authorized and may proceed to execution.
    Allowed,

    /// Action is denied due to invariant violation, scope mismatch, or expired grant.
    Denied { reason: String },

    /// Action is valid but requires explicit human/founder approval before execution.
    RequiresApproval { risk: RiskClass, reason: String },
}

impl AuthorizationDecision {
    #[inline]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    #[inline]
    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Denied { .. })
    }

    #[inline]
    pub fn requires_approval(&self) -> bool {
        matches!(self, Self::RequiresApproval { .. })
    }
}

/// Governance engine that validates requested operations against grants and constitutional rules.
#[derive(Debug, Default, Clone)]
pub struct GovernanceEngine;

impl GovernanceEngine {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates an execution attempt against the supplied TaskGrant, target scope, and action digest.
    pub fn authorize_tool_execution(
        &self,
        grant: &TaskGrant,
        requested_capability: &Capability,
        action_digest: &ActionDigest,
        target_client_scope: Option<&ClientScope>,
        now: DateTime<Utc>,
        approval_actor: Option<&ActorIdentity>,
    ) -> AuthorizationDecision {
        // Rule 1: Grant validity check (status, expiration, call limit)
        if !grant.is_valid_at(now) {
            return AuthorizationDecision::Denied {
                reason: format!(
                    "Grant '{}' is expired, inactive, or exhausted",
                    grant.grant_id
                ),
            };
        }

        // Rule 2: Capability check
        if !grant.has_capability(requested_capability) {
            return AuthorizationDecision::Denied {
                reason: format!(
                    "Grant does not contain capability '{}'",
                    requested_capability
                ),
            };
        }

        // Rule 3: Exact Action Digest Binding (TOCTOU protection)
        if !grant.matches_digest(action_digest) {
            return AuthorizationDecision::Denied {
                reason: format!(
                    "Action digest mismatch: grant bound to '{}', execution requested '{}'",
                    grant.action_digest, action_digest
                ),
            };
        }

        // Rule 4: Client Scope Isolation (Cross-tenant / Cross-client protection)
        if let Some(target_scope) = target_client_scope {
            match &grant.client_scope {
                Some(grant_scope) if grant_scope == target_scope => {
                    // Scope matches
                }
                Some(grant_scope) => {
                    return AuthorizationDecision::Denied {
                        reason: format!(
                            "Client scope isolation violation: grant scope '{}' does not match target scope '{}'",
                            grant_scope, target_scope
                        ),
                    };
                }
                None => {
                    return AuthorizationDecision::Denied {
                        reason: format!(
                            "Client scope violation: target client '{}' requested under unscoped grant",
                            target_scope
                        ),
                    };
                }
            }
        }

        // Rule 5: Risk class and Approval Mandates
        match grant.risk_class {
            RiskClass::R0InternalRead | RiskClass::R1ReversibleLowImpact => {
                // Low risk actions execute automatically with valid grant
                AuthorizationDecision::Allowed
            }
            RiskClass::R2ExternalCommunication => {
                // R2 requires approval unless pre-approved in grant
                if grant.approval_reference.is_some() {
                    AuthorizationDecision::Allowed
                } else {
                    AuthorizationDecision::RequiresApproval {
                        risk: RiskClass::R2ExternalCommunication,
                        reason: "External communication requires approval confirmation".to_string(),
                    }
                }
            }
            RiskClass::R3BusinessCommitment => {
                // R3 requires human approval decision
                if grant.approval_reference.is_some() {
                    AuthorizationDecision::Allowed
                } else {
                    AuthorizationDecision::RequiresApproval {
                        risk: RiskClass::R3BusinessCommitment,
                        reason: "Business commitment (pricing/quote/deadline) requires authorized human approval".to_string(),
                    }
                }
            }
            RiskClass::R4IrreversibleCritical => {
                // R4 strictly requires explicit founder authorization
                if let Some(approver) = approval_actor {
                    if approver.actor_category.is_founder() && grant.approval_reference.is_some() {
                        AuthorizationDecision::Allowed
                    } else {
                        AuthorizationDecision::Denied {
                            reason: "R4 action strictly requires explicit Founder authorization"
                                .to_string(),
                        }
                    }
                } else {
                    AuthorizationDecision::RequiresApproval {
                        risk: RiskClass::R4IrreversibleCritical,
                        reason: "R4 irreversible action requires explicit Founder approval"
                            .to_string(),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use cos_contracts::{ActionDigest, Capability, TaskGrant};
    use cos_domain::{
        AgentId, ApprovalId, ClientScope, GrantId, GrantStatus, RiskClass, SubjectId, TaskId,
        TenantId, TraceId,
    };
    use cos_identity::ActorIdentity;

    fn sample_grant(
        risk_class: RiskClass,
        client_scope: Option<ClientScope>,
        approval_ref: Option<ApprovalId>,
    ) -> (TaskGrant, ActionDigest) {
        let digest = ActionDigest::compute(b"action_payload_123");
        let now = Utc::now();

        let grant = TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id: TaskId::new_v7(),
            agent_id: AgentId::new_v7(),
            subject_id: SubjectId::new_v7(),
            role: "sales".to_string(),
            allowed_capabilities: vec![Capability::new("crm_write")],
            client_scope,
            project_scope: None,
            tenant_scope: TenantId::new_v7(),
            risk_class,
            approval_reference: approval_ref,
            action_digest: digest,
            policy_version: 1,
            grant_status: GrantStatus::Active,
            issued_at: now,
            expires_at: now + Duration::minutes(10),
            max_calls: 5,
            issued_by: ActorIdentity::system("governance"),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        };

        (grant, digest)
    }

    #[test]
    fn test_r0_allowed_without_approval() {
        let (grant, digest) = sample_grant(RiskClass::R0InternalRead, None, None);
        let gov = GovernanceEngine::new();
        let decision = gov.authorize_tool_execution(
            &grant,
            &Capability::new("crm_write"),
            &digest,
            None,
            Utc::now(),
            None,
        );
        assert!(decision.is_allowed());
    }

    #[test]
    fn test_digest_tampering_denied() {
        let (grant, _) = sample_grant(RiskClass::R0InternalRead, None, None);
        let tampered_digest = ActionDigest::compute(b"tampered_payload");
        let gov = GovernanceEngine::new();
        let decision = gov.authorize_tool_execution(
            &grant,
            &Capability::new("crm_write"),
            &tampered_digest,
            None,
            Utc::now(),
            None,
        );
        assert!(decision.is_denied());
    }

    #[test]
    fn test_client_scope_isolation() {
        let client_a = ClientScope::new_v7();
        let client_b = ClientScope::new_v7();

        let (grant, digest) = sample_grant(RiskClass::R1ReversibleLowImpact, Some(client_a), None);
        let gov = GovernanceEngine::new();

        // Accessing Client A under Client A grant -> Allowed
        let decision = gov.authorize_tool_execution(
            &grant,
            &Capability::new("crm_write"),
            &digest,
            Some(&client_a),
            Utc::now(),
            None,
        );
        assert!(decision.is_allowed());

        // Accessing Client B under Client A grant -> Strictly Denied!
        let cross_client_decision = gov.authorize_tool_execution(
            &grant,
            &Capability::new("crm_write"),
            &digest,
            Some(&client_b),
            Utc::now(),
            None,
        );
        assert!(cross_client_decision.is_denied());
    }

    #[test]
    fn test_r3_requires_approval_until_approved() {
        let (unapproved_grant, digest) = sample_grant(RiskClass::R3BusinessCommitment, None, None);
        let gov = GovernanceEngine::new();

        let decision = gov.authorize_tool_execution(
            &unapproved_grant,
            &Capability::new("crm_write"),
            &digest,
            None,
            Utc::now(),
            None,
        );
        assert!(decision.requires_approval());

        // With approval reference -> Allowed
        let (approved_grant, _) = sample_grant(
            RiskClass::R3BusinessCommitment,
            None,
            Some(ApprovalId::new_v7()),
        );
        let approved_decision = gov.authorize_tool_execution(
            &approved_grant,
            &Capability::new("crm_write"),
            &digest,
            None,
            Utc::now(),
            None,
        );
        assert!(approved_decision.is_allowed());
    }

    #[test]
    fn test_r4_strictly_requires_founder() {
        let (approved_grant, digest) = sample_grant(
            RiskClass::R4IrreversibleCritical,
            None,
            Some(ApprovalId::new_v7()),
        );
        let gov = GovernanceEngine::new();

        // Non-founder approver -> Denied
        let non_founder = ActorIdentity::agent(SubjectId::new_v7(), "sales");
        let decision = gov.authorize_tool_execution(
            &approved_grant,
            &Capability::new("crm_write"),
            &digest,
            None,
            Utc::now(),
            Some(&non_founder),
        );
        assert!(decision.is_denied());

        // Founder approver -> Allowed
        let founder = ActorIdentity::founder(SubjectId::new_v7(), None);
        let founder_decision = gov.authorize_tool_execution(
            &approved_grant,
            &Capability::new("crm_write"),
            &digest,
            None,
            Utc::now(),
            Some(&founder),
        );
        assert!(founder_decision.is_allowed());
    }
}
