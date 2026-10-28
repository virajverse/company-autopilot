use cos_approval::ApprovalRequest;
use cos_audit::{AuditEvent, AuditWriterPort};
use cos_domain::ApprovalId;
use cos_goals::{Goal, GoalId};
use cos_llm::LlmGatewayConfig;
use cos_memory::{MemoryNamespace, MemoryRecord};
use cos_outcomes::OutcomeRecord;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::PersistenceError;
use crate::file_store::FileStore;
use crate::sqlite_store::SqliteStore;
use crate::traits::{
    ApprovalRepository, AuditRepository, GatewayConfigRepository, GoalRepository, MemoryRepository,
    OutcomeRepository,
};

/// Unified persistence engine abstracting over FileStore and high-concurrency embedded SqliteStore.
///
/// Both backends run natively on CPU with zero external dependencies.
/// - `Sqlite`: Recommended for production, multi-agent concurrency, and high-frequency audit logs (WAL mode).
/// - `File`: Lightweight plain JSON file storage for zero-dependency inspection and portable environments.
#[derive(Clone)]
pub enum PersistenceEngine {
    File(Arc<FileStore>),
    Sqlite(Arc<SqliteStore>),
}

impl PersistenceEngine {
    /// Opens or connects to SQLite database at the given path.
    pub fn sqlite(path: impl Into<std::path::PathBuf>) -> Result<Self, PersistenceError> {
        let store = SqliteStore::open(path)?;
        Ok(Self::Sqlite(Arc::new(store)))
    }

    /// Initializes an in-memory SQLite store for fast unit/integration testing.
    pub fn sqlite_in_memory() -> Result<Self, PersistenceError> {
        let store = SqliteStore::in_memory()?;
        Ok(Self::Sqlite(Arc::new(store)))
    }

    /// Opens or creates file-based storage at the given directory.
    pub fn file(dir: impl Into<std::path::PathBuf>) -> Result<Self, PersistenceError> {
        let store = FileStore::new(dir)?;
        Ok(Self::File(Arc::new(store)))
    }
}

impl From<Arc<FileStore>> for PersistenceEngine {
    fn from(s: Arc<FileStore>) -> Self {
        Self::File(s)
    }
}

impl From<Arc<SqliteStore>> for PersistenceEngine {
    fn from(s: Arc<SqliteStore>) -> Self {
        Self::Sqlite(s)
    }
}

impl From<FileStore> for PersistenceEngine {
    fn from(s: FileStore) -> Self {
        Self::File(Arc::new(s))
    }
}

impl From<SqliteStore> for PersistenceEngine {
    fn from(s: SqliteStore) -> Self {
        Self::Sqlite(Arc::new(s))
    }
}

impl MemoryRepository for PersistenceEngine {
    fn save_record(&self, record: &MemoryRecord) -> Result<(), PersistenceError> {
        match self {
            Self::File(s) => s.save_record(record),
            Self::Sqlite(s) => s.save_record(record),
        }
    }

    fn get_record(
        &self,
        namespace: &MemoryNamespace,
        key: &str,
    ) -> Result<Option<MemoryRecord>, PersistenceError> {
        match self {
            Self::File(s) => s.get_record(namespace, key),
            Self::Sqlite(s) => s.get_record(namespace, key),
        }
    }

    fn list_records(
        &self,
        namespace: &MemoryNamespace,
    ) -> Result<Vec<MemoryRecord>, PersistenceError> {
        match self {
            Self::File(s) => s.list_records(namespace),
            Self::Sqlite(s) => s.list_records(namespace),
        }
    }

    fn delete_record(
        &self,
        namespace: &MemoryNamespace,
        key: &str,
    ) -> Result<bool, PersistenceError> {
        match self {
            Self::File(s) => s.delete_record(namespace, key),
            Self::Sqlite(s) => s.delete_record(namespace, key),
        }
    }

    fn search_semantic(
        &self,
        namespace: &MemoryNamespace,
        query_embedding: &[f32],
        top_k: usize,
        min_similarity: f32,
    ) -> Result<Vec<(MemoryRecord, f32)>, PersistenceError> {
        match self {
            Self::File(s) => s.search_semantic(namespace, query_embedding, top_k, min_similarity),
            Self::Sqlite(s) => s.search_semantic(namespace, query_embedding, top_k, min_similarity),
        }
    }
}

impl ApprovalRepository for PersistenceEngine {
    fn save_approval(&self, request: &ApprovalRequest) -> Result<(), PersistenceError> {
        match self {
            Self::File(s) => s.save_approval(request),
            Self::Sqlite(s) => s.save_approval(request),
        }
    }

    fn get_approval(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>, PersistenceError> {
        match self {
            Self::File(s) => s.get_approval(id),
            Self::Sqlite(s) => s.get_approval(id),
        }
    }

    fn list_pending(&self) -> Result<Vec<ApprovalRequest>, PersistenceError> {
        match self {
            Self::File(s) => s.list_pending(),
            Self::Sqlite(s) => s.list_pending(),
        }
    }

    fn mark_consumed(&self, id: &ApprovalId) -> Result<(), PersistenceError> {
        match self {
            Self::File(s) => s.mark_consumed(id),
            Self::Sqlite(s) => s.mark_consumed(id),
        }
    }
}

impl AuditRepository for PersistenceEngine {
    fn append_event(&self, event: &AuditEvent) -> Result<(), PersistenceError> {
        match self {
            Self::File(s) => s.append_event(event),
            Self::Sqlite(s) => s.append_event(event),
        }
    }

    fn list_events(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, PersistenceError> {
        match self {
            Self::File(s) => s.list_events(offset, limit),
            Self::Sqlite(s) => s.list_events(offset, limit),
        }
    }

    fn count(&self) -> Result<usize, PersistenceError> {
        match self {
            Self::File(s) => s.count(),
            Self::Sqlite(s) => s.count(),
        }
    }
}

impl GoalRepository for PersistenceEngine {
    fn save_goal(&self, goal: &Goal) -> Result<(), PersistenceError> {
        match self {
            Self::File(s) => s.save_goal(goal),
            Self::Sqlite(s) => s.save_goal(goal),
        }
    }

    fn get_goal(&self, id: &GoalId) -> Result<Option<Goal>, PersistenceError> {
        match self {
            Self::File(s) => s.get_goal(id),
            Self::Sqlite(s) => s.get_goal(id),
        }
    }

    fn list_all(&self) -> Result<Vec<Goal>, PersistenceError> {
        match self {
            Self::File(s) => GoalRepository::list_all(&**s),
            Self::Sqlite(s) => GoalRepository::list_all(&**s),
        }
    }

    fn delete_goal(&self, id: &GoalId) -> Result<bool, PersistenceError> {
        match self {
            Self::File(s) => s.delete_goal(id),
            Self::Sqlite(s) => s.delete_goal(id),
        }
    }
}

impl OutcomeRepository for PersistenceEngine {
    fn save_outcome(&self, outcome: &OutcomeRecord) -> Result<(), PersistenceError> {
        match self {
            Self::File(s) => s.save_outcome(outcome),
            Self::Sqlite(s) => s.save_outcome(outcome),
        }
    }

    fn get_outcome(&self, id: &Uuid) -> Result<Option<OutcomeRecord>, PersistenceError> {
        match self {
            Self::File(s) => s.get_outcome(id),
            Self::Sqlite(s) => s.get_outcome(id),
        }
    }

    fn list_by_goal(&self, goal_id: &GoalId) -> Result<Vec<OutcomeRecord>, PersistenceError> {
        match self {
            Self::File(s) => s.list_by_goal(goal_id),
            Self::Sqlite(s) => s.list_by_goal(goal_id),
        }
    }

    fn list_all(&self) -> Result<Vec<OutcomeRecord>, PersistenceError> {
        match self {
            Self::File(s) => OutcomeRepository::list_all(&**s),
            Self::Sqlite(s) => OutcomeRepository::list_all(&**s),
        }
    }
}

impl GatewayConfigRepository for PersistenceEngine {
    fn load_config(&self) -> Result<LlmGatewayConfig, PersistenceError> {
        match self {
            Self::File(s) => s.load_config(),
            Self::Sqlite(s) => s.load_config(),
        }
    }

    fn save_config(&self, config: &LlmGatewayConfig) -> Result<(), PersistenceError> {
        match self {
            Self::File(s) => s.save_config(config),
            Self::Sqlite(s) => s.save_config(config),
        }
    }
}

impl AuditWriterPort for PersistenceEngine {
    fn record(&self, event: AuditEvent) -> Result<(), String> {
        match self {
            Self::File(s) => s.record(event),
            Self::Sqlite(s) => s.record(event),
        }
    }
}
