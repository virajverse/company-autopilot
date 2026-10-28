use crate::error::PersistenceError;
use cos_approval::ApprovalRequest;
use cos_audit::AuditEvent;
use cos_domain::ApprovalId;
use cos_goals::{Goal, GoalId};
use cos_llm::LlmGatewayConfig;
use cos_memory::{MemoryNamespace, MemoryRecord};
use cos_outcomes::OutcomeRecord;
use uuid::Uuid;

/// Trait for durable persistence of scoped memory records.
pub trait MemoryRepository: Send + Sync {
    fn save_record(&self, record: &MemoryRecord) -> Result<(), PersistenceError>;
    fn get_record(
        &self,
        namespace: &MemoryNamespace,
        key: &str,
    ) -> Result<Option<MemoryRecord>, PersistenceError>;
    fn list_records(
        &self,
        namespace: &MemoryNamespace,
    ) -> Result<Vec<MemoryRecord>, PersistenceError>;
    fn delete_record(
        &self,
        namespace: &MemoryNamespace,
        key: &str,
    ) -> Result<bool, PersistenceError>;

    /// Searches records within a namespace using semantic vector embeddings and CPU cosine similarity.
    fn search_semantic(
        &self,
        namespace: &MemoryNamespace,
        query_embedding: &[f32],
        top_k: usize,
        min_similarity: f32,
    ) -> Result<Vec<(MemoryRecord, f32)>, PersistenceError> {
        let records = self.list_records(namespace)?;
        let mut scored: Vec<(MemoryRecord, f32)> = records
            .into_iter()
            .filter_map(|r| {
                if let Some(ref emb) = r.embedding {
                    let sim = cos_memory::cosine_similarity(query_embedding, emb);
                    Some((r, sim))
                } else {
                    None
                }
            })
            .filter(|(_, sim)| *sim >= min_similarity)
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        Ok(scored)
    }
}

/// Trait for durable persistence of human approval requests.
pub trait ApprovalRepository: Send + Sync {
    fn save_approval(&self, request: &ApprovalRequest) -> Result<(), PersistenceError>;
    fn get_approval(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>, PersistenceError>;
    fn list_pending(&self) -> Result<Vec<ApprovalRequest>, PersistenceError>;
    fn mark_consumed(&self, id: &ApprovalId) -> Result<(), PersistenceError>;
}

/// Trait for durable append-only persistence of audit events.
pub trait AuditRepository: Send + Sync {
    fn append_event(&self, event: &AuditEvent) -> Result<(), PersistenceError>;
    fn list_events(&self, offset: usize, limit: usize)
        -> Result<Vec<AuditEvent>, PersistenceError>;
    fn count(&self) -> Result<usize, PersistenceError>;
}

/// Trait for durable persistence of the Goal Forest.
pub trait GoalRepository: Send + Sync {
    fn save_goal(&self, goal: &Goal) -> Result<(), PersistenceError>;
    fn get_goal(&self, id: &GoalId) -> Result<Option<Goal>, PersistenceError>;
    fn list_all(&self) -> Result<Vec<Goal>, PersistenceError>;
    fn delete_goal(&self, id: &GoalId) -> Result<bool, PersistenceError>;
}

/// Trait for durable persistence of task outcome records.
pub trait OutcomeRepository: Send + Sync {
    fn save_outcome(&self, outcome: &OutcomeRecord) -> Result<(), PersistenceError>;
    fn get_outcome(&self, id: &Uuid) -> Result<Option<OutcomeRecord>, PersistenceError>;
    fn list_by_goal(&self, goal_id: &GoalId) -> Result<Vec<OutcomeRecord>, PersistenceError>;
    fn list_all(&self) -> Result<Vec<OutcomeRecord>, PersistenceError>;
}

/// Trait for durable persistence of the LLM Gateway Configuration.
pub trait GatewayConfigRepository: Send + Sync {
    fn load_config(&self) -> Result<LlmGatewayConfig, PersistenceError>;
    fn save_config(&self, config: &LlmGatewayConfig) -> Result<(), PersistenceError>;
}
