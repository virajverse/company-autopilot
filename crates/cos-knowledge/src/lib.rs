//! `cos-knowledge` manages epistemic status, evidence provenance, and contradiction lifecycle.
//!
//! # Invariants:
//! 1. Memory is storage, Knowledge is truth status and confidence.
//! 2. Unverified claims remain Candidate or Observed until verified by evidence.
//! 3. Contradicted statements trigger explicit Investigation rather than silent overwrite.

use cos_domain::KnowledgeLifecycle;
use cos_memory::{MemoryError, MemoryRecord, MemoryStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KnowledgeError {
    #[error("Memory error: {0}")]
    Memory(#[from] MemoryError),

    #[error("Invalid epistemic transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: KnowledgeLifecycle,
        to: KnowledgeLifecycle,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeEvaluation {
    pub is_operationally_safe: bool,
    pub confidence_score: f32,
    pub status: KnowledgeLifecycle,
}

pub struct KnowledgeAuthority;

impl KnowledgeAuthority {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates whether a memory record is sufficiently verified for operational reasoning
    /// using a context-specific or policy-defined minimum confidence threshold.
    pub fn evaluate_record(
        &self,
        record: &MemoryRecord,
        min_confidence: f32,
    ) -> KnowledgeEvaluation {
        KnowledgeEvaluation {
            is_operationally_safe: record.lifecycle_status.is_trusted_or_verified()
                && record.confidence >= min_confidence.clamp(0.0, 1.0),
            confidence_score: record.confidence,
            status: record.lifecycle_status,
        }
    }

    /// Verifies and promotes a Candidate memory record to Verified status.
    pub fn promote_to_verified(
        &self,
        record: &mut MemoryRecord,
        new_confidence: f32,
        store: &MemoryStore,
    ) -> Result<(), KnowledgeError> {
        record
            .lifecycle_status
            .transition_to(KnowledgeLifecycle::Verified)
            .map_err(|_| KnowledgeError::InvalidTransition {
                from: record.lifecycle_status,
                to: KnowledgeLifecycle::Verified,
            })?;

        record.confidence = new_confidence.clamp(0.0, 1.0);
        record.version += 1;
        record.updated_at = chrono::Utc::now();

        store.store(record.clone())?;
        Ok(())
    }
}

impl Default for KnowledgeAuthority {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cos_identity::ActorIdentity;
    use cos_memory::{MemoryKind, MemoryNamespace, ProvenanceType};

    #[test]
    fn test_knowledge_evaluation_and_promotion() {
        let store = MemoryStore::new();
        let authority = KnowledgeAuthority::new();
        let author = ActorIdentity::system("knowledge_evaluator");

        let mut record = MemoryRecord::new(
            MemoryNamespace::Company,
            MemoryKind::Guideline,
            "response_policy",
            serde_json::json!({ "max_wait_hours": 2 }),
            KnowledgeLifecycle::Observed,
            0.5,
            ProvenanceType::Observed,
            author,
        );
        store.store(record.clone()).unwrap();

        // Not yet safe for critical operations (tested against 0.7 requirement)
        let eval = authority.evaluate_record(&record, 0.7);
        assert!(!eval.is_operationally_safe);

        // Promote to Verified with 0.95 confidence
        authority
            .promote_to_verified(&mut record, 0.95, &store)
            .unwrap();
        assert_eq!(record.lifecycle_status, KnowledgeLifecycle::Verified);

        let eval_promoted = authority.evaluate_record(&record, 0.7);
        assert!(eval_promoted.is_operationally_safe);
        assert_eq!(eval_promoted.confidence_score, 0.95);

        // Even with high confidence, if context requires 0.99, it evaluates appropriately
        let strict_eval = authority.evaluate_record(&record, 0.99);
        assert!(!strict_eval.is_operationally_safe);
    }
}
