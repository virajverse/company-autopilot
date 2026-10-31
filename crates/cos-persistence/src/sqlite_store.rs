use chrono::Utc;
use cos_approval::ApprovalRequest;
use cos_audit::{AuditEvent, AuditWriterPort};
use cos_domain::ApprovalId;
use cos_goals::{Goal, GoalId};
use cos_llm::LlmGatewayConfig;
use cos_memory::{MemoryNamespace, MemoryRecord};
use cos_outcomes::OutcomeRecord;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

use crate::error::PersistenceError;
use crate::traits::{
    ApprovalRepository, AuditRepository, GatewayConfigRepository, GoalRepository, MemoryRepository,
    OutcomeRepository,
};

/// High-performance embedded SQLite storage engine running entirely on CPU.
///
/// Features:
/// - Write-Ahead Logging (WAL) for sub-millisecond concurrent execution.
/// - Full ACID transaction safety with zero external database dependencies.
/// - Implements all core repository ports identically to FileStore.
pub struct SqliteStore {
    conn: Mutex<Connection>,
    pub db_path: PathBuf,
}

impl SqliteStore {
    /// Opens or creates an embedded SQLite database at the specified path.
    pub fn open(db_path: impl Into<PathBuf>) -> Result<Self, PersistenceError> {
        let path = db_path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path).map_err(|e| {
            PersistenceError::StorageError(format!("Failed to open SQLite database: {}", e))
        })?;

        // 1. Optimize SQLite for CPU speed & concurrent read/write via WAL mode
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(|e| {
            PersistenceError::StorageError(format!("Failed to configure SQLite pragmas: {}", e))
        })?;

        let store = Self {
            conn: Mutex::new(conn),
            db_path: path,
        };

        // 2. Initialize tables and indices
        store.init_schema()?;

        Ok(store)
    }

    /// Creates an in-memory SQLite store for unit testing.
    pub fn in_memory() -> Result<Self, PersistenceError> {
        let conn = Connection::open_in_memory().map_err(|e| {
            PersistenceError::StorageError(format!("Failed to open in-memory SQLite: {}", e))
        })?;

        let store = Self {
            conn: Mutex::new(conn),
            db_path: PathBuf::from(":memory:"),
        };

        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), PersistenceError> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory_records (
                id TEXT PRIMARY KEY,
                namespace_json TEXT NOT NULL,
                key TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(namespace_json, key)
            );
            CREATE INDEX IF NOT EXISTS idx_mem_ns_key ON memory_records(namespace_json, key);

            CREATE TABLE IF NOT EXISTS approvals (
                approval_id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_approvals_status ON approvals(status);

            CREATE TABLE IF NOT EXISTS audit_events (
                seq_id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                data_json TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_audit_time ON audit_events(timestamp);

            CREATE TABLE IF NOT EXISTS goals (
                goal_id TEXT PRIMARY KEY,
                parent_goal_id TEXT,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_goals_parent ON goals(parent_goal_id);

            CREATE TABLE IF NOT EXISTS outcomes (
                outcome_id TEXT PRIMARY KEY,
                goal_id TEXT,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_outcomes_goal ON outcomes(goal_id);

            CREATE TABLE IF NOT EXISTS gateway_config (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )
        .map_err(|e| {
            PersistenceError::StorageError(format!("Failed to initialize SQLite schema: {}", e))
        })?;

        Ok(())
    }
}

impl MemoryRepository for SqliteStore {
    fn save_record(&self, record: &MemoryRecord) -> Result<(), PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let ns_json = serde_json::to_string(&record.namespace)?;
        let data_json = serde_json::to_string(record)?;
        let id_str = record.memory_id.0.to_string();
        let now_str = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO memory_records (id, namespace_json, key, data_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(namespace_json, key) DO UPDATE SET
                data_json = excluded.data_json,
                created_at = excluded.created_at",
            params![id_str, ns_json, record.key, data_json, now_str],
        )
        .map_err(|e| {
            PersistenceError::StorageError(format!("Failed to save memory record: {}", e))
        })?;

        Ok(())
    }

    fn get_record(
        &self,
        namespace: &MemoryNamespace,
        key: &str,
    ) -> Result<Option<MemoryRecord>, PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let ns_json = serde_json::to_string(namespace)?;

        let mut stmt = conn
            .prepare("SELECT data_json FROM memory_records WHERE namespace_json = ?1 AND key = ?2")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut rows = stmt
            .query(params![ns_json, key])
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?
        {
            let data_str: String = row
                .get(0)
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            let record: MemoryRecord = serde_json::from_str(&data_str)?;
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    fn list_records(
        &self,
        namespace: &MemoryNamespace,
    ) -> Result<Vec<MemoryRecord>, PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let ns_json = serde_json::to_string(namespace)?;

        let mut stmt = conn
            .prepare("SELECT data_json FROM memory_records WHERE namespace_json = ?1")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let rows = stmt
            .query_map(params![ns_json], |row| row.get::<_, String>(0))
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut records = Vec::new();
        for data_res in rows {
            let data_str = data_res.map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            if let Ok(rec) = serde_json::from_str::<MemoryRecord>(&data_str) {
                records.push(rec);
            }
        }

        Ok(records)
    }

    fn delete_record(
        &self,
        namespace: &MemoryNamespace,
        key: &str,
    ) -> Result<bool, PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let ns_json = serde_json::to_string(namespace)?;

        let rows_affected = conn
            .execute(
                "DELETE FROM memory_records WHERE namespace_json = ?1 AND key = ?2",
                params![ns_json, key],
            )
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        Ok(rows_affected > 0)
    }
}

impl cos_approval::ApprovalQueryPort for SqliteStore {
    fn get_approval(&self, id: &ApprovalId) -> Option<ApprovalRequest> {
        ApprovalRepository::get_approval(self, id).ok().flatten()
    }
}

impl ApprovalRepository for SqliteStore {
    fn save_approval(&self, request: &ApprovalRequest) -> Result<(), PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let id_str = request.approval_id.0.to_string();
        let status_str = format!("{:?}", request.status);
        let data_json = serde_json::to_string(request)?;
        let now_str = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO approvals (approval_id, status, data_json, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(approval_id) DO UPDATE SET
                status = excluded.status,
                data_json = excluded.data_json",
            params![id_str, status_str, data_json, now_str],
        )
        .map_err(|e| PersistenceError::StorageError(format!("Failed to save approval: {}", e)))?;

        Ok(())
    }

    fn get_approval(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>, PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let id_str = id.0.to_string();

        let mut stmt = conn
            .prepare("SELECT data_json FROM approvals WHERE approval_id = ?1")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut rows = stmt
            .query(params![id_str])
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?
        {
            let data_str: String = row
                .get(0)
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            let req: ApprovalRequest = serde_json::from_str(&data_str)?;
            Ok(Some(req))
        } else {
            Ok(None)
        }
    }

    fn list_pending(&self) -> Result<Vec<ApprovalRequest>, PersistenceError> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT data_json FROM approvals WHERE status = 'Submitted' OR status = 'NeedsClarification'")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut requests = Vec::new();
        for data_res in rows {
            let data_str = data_res.map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            if let Ok(req) = serde_json::from_str::<ApprovalRequest>(&data_str) {
                requests.push(req);
            }
        }

        Ok(requests)
    }

    fn mark_consumed(&self, id: &ApprovalId) -> Result<(), PersistenceError> {
        let mut req = self
            .get_approval(id)?
            .ok_or_else(|| PersistenceError::NotFound(format!("Approval {}", id.0)))?;

        req.consume()
            .map_err(|e| PersistenceError::Conflict(e.to_string()))?;
        self.save_approval(&req)
    }
}

impl AuditRepository for SqliteStore {
    fn append_event(&self, event: &AuditEvent) -> Result<(), PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let event_id = event.audit_id.to_string();
        let event_type = format!("{:?}", event.event_type);
        let data_json = serde_json::to_string(event)?;
        let time_str = event.recorded_at.to_rfc3339();

        conn.execute(
            "INSERT INTO audit_events (event_id, event_type, data_json, timestamp)
             VALUES (?1, ?2, ?3, ?4)",
            params![event_id, event_type, data_json, time_str],
        )
        .map_err(|e| {
            PersistenceError::StorageError(format!("Failed to append audit event: {}", e))
        })?;

        Ok(())
    }

    fn list_events(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, PersistenceError> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT data_json FROM audit_events ORDER BY seq_id ASC LIMIT ?1 OFFSET ?2")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let rows = stmt
            .query_map(params![limit as i64, offset as i64], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut events = Vec::new();
        for data_res in rows {
            let data_str = data_res.map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            if let Ok(ev) = serde_json::from_str::<AuditEvent>(&data_str) {
                events.push(ev);
            }
        }

        Ok(events)
    }

    fn count(&self) -> Result<usize, PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        Ok(count as usize)
    }
}

impl GoalRepository for SqliteStore {
    fn save_goal(&self, goal: &Goal) -> Result<(), PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let goal_id = goal.id.0.to_string();
        let parent_id = goal.parent_goal_id.map(|p| p.0.to_string());
        let data_json = serde_json::to_string(goal)?;
        let now_str = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO goals (goal_id, parent_goal_id, data_json, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(goal_id) DO UPDATE SET
                parent_goal_id = excluded.parent_goal_id,
                data_json = excluded.data_json",
            params![goal_id, parent_id, data_json, now_str],
        )
        .map_err(|e| PersistenceError::StorageError(format!("Failed to save goal: {}", e)))?;

        Ok(())
    }

    fn get_goal(&self, id: &GoalId) -> Result<Option<Goal>, PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let goal_id = id.0.to_string();

        let mut stmt = conn
            .prepare("SELECT data_json FROM goals WHERE goal_id = ?1")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut rows = stmt
            .query(params![goal_id])
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?
        {
            let data_str: String = row
                .get(0)
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            let goal: Goal = serde_json::from_str(&data_str)?;
            Ok(Some(goal))
        } else {
            Ok(None)
        }
    }

    fn list_all(&self) -> Result<Vec<Goal>, PersistenceError> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT data_json FROM goals")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut goals = Vec::new();
        for data_res in rows {
            let data_str = data_res.map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            if let Ok(g) = serde_json::from_str::<Goal>(&data_str) {
                goals.push(g);
            }
        }

        Ok(goals)
    }

    fn delete_goal(&self, id: &GoalId) -> Result<bool, PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let goal_id = id.0.to_string();

        let rows = conn
            .execute("DELETE FROM goals WHERE goal_id = ?1", params![goal_id])
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        Ok(rows > 0)
    }
}

impl OutcomeRepository for SqliteStore {
    fn save_outcome(&self, outcome: &OutcomeRecord) -> Result<(), PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let outcome_id = outcome.id.to_string();
        let goal_id = outcome.goal_id.map(|g| g.0.to_string());
        let data_json = serde_json::to_string(outcome)?;
        let now_str = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO outcomes (outcome_id, goal_id, data_json, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(outcome_id) DO UPDATE SET
                data_json = excluded.data_json",
            params![outcome_id, goal_id, data_json, now_str],
        )
        .map_err(|e| PersistenceError::StorageError(format!("Failed to save outcome: {}", e)))?;

        Ok(())
    }

    fn get_outcome(&self, id: &Uuid) -> Result<Option<OutcomeRecord>, PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let id_str = id.to_string();

        let mut stmt = conn
            .prepare("SELECT data_json FROM outcomes WHERE outcome_id = ?1")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut rows = stmt
            .query(params![id_str])
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?
        {
            let data_str: String = row
                .get(0)
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            let out: OutcomeRecord = serde_json::from_str(&data_str)?;
            Ok(Some(out))
        } else {
            Ok(None)
        }
    }

    fn list_by_goal(&self, goal_id: &GoalId) -> Result<Vec<OutcomeRecord>, PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let goal_str = goal_id.0.to_string();

        let mut stmt = conn
            .prepare("SELECT data_json FROM outcomes WHERE goal_id = ?1")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let rows = stmt
            .query_map(params![goal_str], |row| row.get::<_, String>(0))
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut outcomes = Vec::new();
        for data_res in rows {
            let data_str = data_res.map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            if let Ok(out) = serde_json::from_str::<OutcomeRecord>(&data_str) {
                outcomes.push(out);
            }
        }

        Ok(outcomes)
    }

    fn list_all(&self) -> Result<Vec<OutcomeRecord>, PersistenceError> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT data_json FROM outcomes")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut outcomes = Vec::new();
        for data_res in rows {
            let data_str = data_res.map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            if let Ok(out) = serde_json::from_str::<OutcomeRecord>(&data_str) {
                outcomes.push(out);
            }
        }

        Ok(outcomes)
    }
}

impl GatewayConfigRepository for SqliteStore {
    fn load_config(&self) -> Result<LlmGatewayConfig, PersistenceError> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT data_json FROM gateway_config WHERE id = 1")
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let mut rows = stmt
            .query([])
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?
        {
            let data_str: String = row
                .get(0)
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
            let cfg: LlmGatewayConfig = serde_json::from_str(&data_str)?;
            Ok(cfg)
        } else {
            Ok(LlmGatewayConfig::default())
        }
    }

    fn save_config(&self, config: &LlmGatewayConfig) -> Result<(), PersistenceError> {
        let conn = self.conn.lock().unwrap();
        let data_json = serde_json::to_string(config)?;
        let now_str = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO gateway_config (id, data_json, updated_at)
             VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                data_json = excluded.data_json,
                updated_at = excluded.updated_at",
            params![data_json, now_str],
        )
        .map_err(|e| {
            PersistenceError::StorageError(format!("Failed to save gateway config: {}", e))
        })?;

        Ok(())
    }
}

impl AuditWriterPort for SqliteStore {
    fn record(&self, event: AuditEvent) -> Result<(), String> {
        self.append_event(&event).map_err(|e| e.to_string())
    }
}
