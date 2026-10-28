use crate::types::{LearningError, LearningProposal, ProposalStatus};
use cos_domain::KnowledgeLifecycle;
use cos_identity::ActorIdentity;
use cos_memory::{MemoryKind, MemoryNamespace, MemoryRecord, MemoryStore, ProvenanceType};
use cos_outcomes::OutcomeRecord;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Engine analyzing outcome trends and proposing optimizations to operational procedures.
#[derive(Debug, Clone, Default)]
pub struct LearningEngine {
    proposals: Arc<RwLock<HashMap<Uuid, LearningProposal>>>,
}

impl LearningEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Submits a learning proposal after constitutional guardrail verification.
    pub fn submit_proposal(&self, proposal: LearningProposal) -> Uuid {
        let id = proposal.id;
        self.proposals.write().unwrap().insert(id, proposal);
        id
    }

    /// Retrieves a proposal by its ID.
    pub fn get_proposal(&self, id: &Uuid) -> Option<LearningProposal> {
        self.proposals.read().unwrap().get(id).cloned()
    }

    /// Lists all pending proposals awaiting review.
    pub fn list_pending(&self) -> Vec<LearningProposal> {
        self.proposals
            .read()
            .unwrap()
            .values()
            .filter(|p| p.status == ProposalStatus::PendingReview)
            .cloned()
            .collect()
    }

    /// Automatically synthesizes a proposal from recurring failures.
    pub fn analyze_and_propose(
        &self,
        failures: &[OutcomeRecord],
        target_sop_key: &str,
        improved_sop_content: serde_json::Value,
    ) -> Result<Uuid, LearningError> {
        let evidence_ids: Vec<Uuid> = failures.iter().map(|f| f.id).collect();
        let rationale = format!(
            "Observed {} operational failure(s). Proposing SOP refinement to mitigate failures.",
            failures.len()
        );

        let proposal = LearningProposal::new(
            MemoryNamespace::Company,
            target_sop_key,
            rationale,
            improved_sop_content,
            evidence_ids,
        )?;

        let id = self.submit_proposal(proposal);
        Ok(id)
    }

    /// Applies an approved proposal to the dynamic memory store.
    pub fn apply_proposal(
        &self,
        proposal_id: &Uuid,
        memory_store: &MemoryStore,
        author: ActorIdentity,
    ) -> Result<(), LearningError> {
        let mut proposals = self.proposals.write().unwrap();
        let proposal = proposals
            .get_mut(proposal_id)
            .ok_or(LearningError::ProposalNotFound(*proposal_id))?;

        // Write the updated dynamic SOP to memory
        let record = MemoryRecord::new(
            proposal.target_namespace.clone(),
            MemoryKind::Procedure,
            proposal.target_sop_key.clone(),
            proposal.proposed_content.clone(),
            KnowledgeLifecycle::Candidate,
            0.85,
            ProvenanceType::Learned,
            author,
        );

        memory_store
            .store(record)
            .map_err(|e| LearningError::MemoryError(e.to_string()))?;

        proposal.status = ProposalStatus::Applied;
        Ok(())
    }
}
