//! `cos-contracts` defines shared envelopes, tracing contexts, cryptographic action digests,
//! and canonical capability grants.

pub mod digest;
pub mod envelope;
pub mod grant;
pub mod trace;

pub use digest::ActionDigest;
pub use envelope::{CommandEnvelope, EventEnvelope};
pub use grant::{Capability, TaskGrant};
pub use trace::TraceContext;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use cos_domain::{
        AgentId, GrantId, GrantStatus, RiskClass, SubjectId, TaskId, TenantId, TraceId,
    };
    use cos_identity::ActorIdentity;

    #[test]
    fn test_task_grant_validity_and_digest_check() {
        let digest = ActionDigest::compute(b"test_action");
        let now = Utc::now();

        let grant = TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id: TaskId::new_v7(),
            agent_id: AgentId::new_v7(),
            subject_id: SubjectId::new_v7(),
            role: "sales".to_string(),
            allowed_capabilities: vec![Capability::new("crm_read")],
            client_scope: None,
            project_scope: None,
            tenant_scope: TenantId::new_v7(),
            risk_class: RiskClass::R0InternalRead,
            approval_reference: None,
            action_digest: digest,
            policy_version: 1,
            grant_status: GrantStatus::Active,
            issued_at: now,
            expires_at: now + Duration::minutes(5),
            max_calls: 3,
            issued_by: ActorIdentity::system("governance"),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        };

        assert!(grant.is_valid_at(now));
        assert!(grant.matches_digest(&digest));
        assert!(grant.has_capability(&Capability::new("crm_read")));
        assert!(!grant.has_capability(&Capability::new("external_send")));

        // Test expired grant
        let future_time = now + Duration::minutes(10);
        assert!(!grant.is_valid_at(future_time));
    }
}
