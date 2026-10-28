use cos_domain::{ClientScope, TaskId};
use cos_identity::ActorIdentity;
use cos_learning::{LearningEngine, LearningError, LearningProposal, ProposalStatus};
use cos_memory::{MemoryNamespace, MemoryStore};
use cos_outcomes::{OutcomeRecord, OutcomeStatus};

#[test]
fn test_constitutional_guardrail_blocks_governance_tampering() {
    let result = LearningProposal::new(
        MemoryNamespace::Company,
        "governance:risk_limits",
        "Attempting to modify security limits",
        serde_json::json!({}),
        vec![],
    );

    assert!(result.is_err());
    match result.unwrap_err() {
        LearningError::ConstitutionalViolation(msg) => {
            assert!(msg.contains("protected constitutional boundary"));
        }
        _ => panic!("Expected ConstitutionalViolation"),
    }
}

#[test]
fn test_client_memory_isolation_guardrail() {
    let client_scope = ClientScope::new_v7();
    let result = LearningProposal::new(
        MemoryNamespace::Client(client_scope),
        "client_private_sop",
        "Attempting to modify client private space",
        serde_json::json!({}),
        vec![],
    );

    assert!(result.is_err());
    match result.unwrap_err() {
        LearningError::ConstitutionalViolation(msg) => {
            assert!(msg.contains("Client memory isolation violation"));
        }
        _ => panic!("Expected ConstitutionalViolation"),
    }
}

#[test]
fn test_proposal_synthesis_and_application() {
    let engine = LearningEngine::new();
    let memory = MemoryStore::new();
    let actor = ActorIdentity::system("outcome-monitor");

    let failure1 = OutcomeRecord::new(
        TaskId::new_v7(),
        OutcomeStatus::Failure,
        "Failed lead qualification",
        0.9,
        actor.clone(),
    );

    let proposal_id = engine
        .analyze_and_propose(
            &[failure1],
            "lead_qualification_sop",
            serde_json::json!({
                "version": 2,
                "retry_limit": 3,
                "timeout_sec": 30
            }),
        )
        .expect("Synthesize proposal");

    let proposal = engine.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::PendingReview);
    assert_eq!(proposal.evidence_outcome_ids.len(), 1);

    // Apply proposal to memory
    engine.apply_proposal(&proposal_id, &memory, actor).unwrap();

    let updated = engine.get_proposal(&proposal_id).unwrap();
    assert_eq!(updated.status, ProposalStatus::Applied);

    let mem_rec = memory
        .get_by_key(&MemoryNamespace::Company, "lead_qualification_sop", None)
        .expect("Query memory")
        .expect("Record exists");

    assert_eq!(mem_rec.content.get("retry_limit").unwrap(), 3);
}
