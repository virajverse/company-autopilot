use crate::error::PersistenceError;
use crate::traits::{
    ApprovalRepository, AuditRepository, GatewayConfigRepository, GoalRepository, MemoryRepository,
    OutcomeRepository,
};
use cos_approval::ApprovalRequest;
use cos_audit::AuditEvent;
use cos_domain::ApprovalId;
use cos_goals::{Goal, GoalId};
use cos_llm::LlmGatewayConfig;
use cos_memory::{MemoryNamespace, MemoryRecord};
use cos_outcomes::OutcomeRecord;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use uuid::Uuid;

/// File-backed atomic crash-safe persistent storage engine.
#[derive(Debug)]
pub struct FileStore {
    base_path: PathBuf,
    lock: RwLock<()>,
}

impl FileStore {
    /// Creates or opens a FileStore directory structure at the given path.
    pub fn new(base_path: impl Into<PathBuf>) -> Result<Self, PersistenceError> {
        let path = base_path.into();
        fs::create_dir_all(&path)?;
        fs::create_dir_all(path.join("memory"))?;
        fs::create_dir_all(path.join("approvals"))?;
        fs::create_dir_all(path.join("audit"))?;
        fs::create_dir_all(path.join("goals"))?;
        fs::create_dir_all(path.join("outcomes"))?;
        fs::create_dir_all(path.join("config"))?;

        Ok(Self {
            base_path: path,
            lock: RwLock::new(()),
        })
    }

    /// Atomically writes serialized data to a file using temporary file swap.
    fn atomic_write(&self, target_path: &Path, content: &[u8]) -> Result<(), PersistenceError> {
        let parent = target_path.parent().unwrap_or(&self.base_path);
        fs::create_dir_all(parent)?;

        let temp_filename = format!(".tmp-{}", Uuid::new_v4());
        let temp_path = parent.join(temp_filename);

        {
            let mut file = File::create(&temp_path)?;
            file.write_all(content)?;
            file.flush()?;
        }

        // On Windows and Unix, fs::rename atomically replaces destination if it exists
        if target_path.exists() {
            let _ = fs::remove_file(target_path);
        }
        fs::rename(&temp_path, target_path)?;

        Ok(())
    }

    fn memory_namespace_dir(&self, namespace: &MemoryNamespace) -> PathBuf {
        let ns_str = match namespace {
            MemoryNamespace::Company => "company".to_string(),
            MemoryNamespace::Client(c) => format!("client_{}", c.0),
            MemoryNamespace::Task(t) => format!("task_{}", t.0),
            MemoryNamespace::AgentWorking(a) => format!("agent_{}", a.0),
        };

        self.base_path.join("memory").join(ns_str)
    }

    fn memory_key_path(&self, namespace: &MemoryNamespace, key: &str) -> PathBuf {
        let safe_key = key
            .replace(':', "_COLON_")
            .replace('/', "_SLASH_")
            .replace('\\', "_BSLASH_");
        self.memory_namespace_dir(namespace)
            .join(format!("{}.json", safe_key))
    }
}

impl MemoryRepository for FileStore {
    fn save_record(&self, record: &MemoryRecord) -> Result<(), PersistenceError> {
        let _guard = self.lock.write().unwrap();
        let path = self.memory_key_path(&record.namespace, &record.key);
        let bytes = serde_json::to_vec_pretty(record)?;
        self.atomic_write(&path, &bytes)
    }

    fn get_record(
        &self,
        namespace: &MemoryNamespace,
        key: &str,
    ) -> Result<Option<MemoryRecord>, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let path = self.memory_key_path(namespace, key);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)?;
        let record: MemoryRecord = serde_json::from_slice(&bytes)?;
        Ok(Some(record))
    }

    fn list_records(
        &self,
        namespace: &MemoryNamespace,
    ) -> Result<Vec<MemoryRecord>, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let dir = self.memory_namespace_dir(namespace);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(bytes) = fs::read(&path) {
                    if let Ok(rec) = serde_json::from_slice::<MemoryRecord>(&bytes) {
                        results.push(rec);
                    }
                }
            }
        }
        Ok(results)
    }

    fn delete_record(
        &self,
        namespace: &MemoryNamespace,
        key: &str,
    ) -> Result<bool, PersistenceError> {
        let _guard = self.lock.write().unwrap();
        let path = self.memory_key_path(namespace, key);
        if path.exists() {
            fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl ApprovalRepository for FileStore {
    fn save_approval(&self, request: &ApprovalRequest) -> Result<(), PersistenceError> {
        let _guard = self.lock.write().unwrap();
        let path = self
            .base_path
            .join("approvals")
            .join(format!("{}.json", request.approval_id.0));
        let bytes = serde_json::to_vec_pretty(request)?;
        self.atomic_write(&path, &bytes)
    }

    fn get_approval(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let path = self
            .base_path
            .join("approvals")
            .join(format!("{}.json", id.0));
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)?;
        let req = serde_json::from_slice(&bytes)?;
        Ok(Some(req))
    }

    fn list_pending(&self) -> Result<Vec<ApprovalRequest>, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let dir = self.base_path.join("approvals");
        let mut results = Vec::new();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(bytes) = fs::read(&path) {
                    if let Ok(req) = serde_json::from_slice::<ApprovalRequest>(&bytes) {
                        if req.status == cos_domain::ApprovalStatus::Submitted {
                            results.push(req);
                        }
                    }
                }
            }
        }
        Ok(results)
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

impl AuditRepository for FileStore {
    fn append_event(&self, event: &AuditEvent) -> Result<(), PersistenceError> {
        let _guard = self.lock.write().unwrap();
        let log_path = self.base_path.join("audit").join("events.jsonl");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        let line = serde_json::to_string(event)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    fn list_events(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let log_path = self.base_path.join("audit").join("events.jsonl");
        if !log_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&log_path)?;
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;

        let mut events = Vec::new();
        for line_res in reader.lines().skip(offset).take(limit) {
            let line = line_res?;
            if let Ok(ev) = serde_json::from_str::<AuditEvent>(&line) {
                events.push(ev);
            }
        }
        Ok(events)
    }

    fn count(&self) -> Result<usize, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let log_path = self.base_path.join("audit").join("events.jsonl");
        if !log_path.exists() {
            return Ok(0);
        }
        let file = File::open(&log_path)?;
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;
        Ok(reader.lines().count())
    }
}

impl GoalRepository for FileStore {
    fn save_goal(&self, goal: &Goal) -> Result<(), PersistenceError> {
        let _guard = self.lock.write().unwrap();
        let path = self
            .base_path
            .join("goals")
            .join(format!("{}.json", goal.id.0));
        let bytes = serde_json::to_vec_pretty(goal)?;
        self.atomic_write(&path, &bytes)
    }

    fn get_goal(&self, id: &GoalId) -> Result<Option<Goal>, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let path = self.base_path.join("goals").join(format!("{}.json", id.0));
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)?;
        let g = serde_json::from_slice(&bytes)?;
        Ok(Some(g))
    }

    fn list_all(&self) -> Result<Vec<Goal>, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let dir = self.base_path.join("goals");
        let mut results = Vec::new();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(bytes) = fs::read(&path) {
                    if let Ok(g) = serde_json::from_slice::<Goal>(&bytes) {
                        results.push(g);
                    }
                }
            }
        }
        Ok(results)
    }

    fn delete_goal(&self, id: &GoalId) -> Result<bool, PersistenceError> {
        let _guard = self.lock.write().unwrap();
        let path = self.base_path.join("goals").join(format!("{}.json", id.0));
        if path.exists() {
            fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl OutcomeRepository for FileStore {
    fn save_outcome(&self, outcome: &OutcomeRecord) -> Result<(), PersistenceError> {
        let _guard = self.lock.write().unwrap();
        let path = self
            .base_path
            .join("outcomes")
            .join(format!("{}.json", outcome.id));
        let bytes = serde_json::to_vec_pretty(outcome)?;
        self.atomic_write(&path, &bytes)
    }

    fn get_outcome(&self, id: &Uuid) -> Result<Option<OutcomeRecord>, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let path = self.base_path.join("outcomes").join(format!("{}.json", id));
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)?;
        let o = serde_json::from_slice(&bytes)?;
        Ok(Some(o))
    }

    fn list_by_goal(&self, goal_id: &GoalId) -> Result<Vec<OutcomeRecord>, PersistenceError> {
        let all = OutcomeRepository::list_all(self)?;
        Ok(all
            .into_iter()
            .filter(|o| o.goal_id == Some(*goal_id))
            .collect())
    }

    fn list_all(&self) -> Result<Vec<OutcomeRecord>, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let dir = self.base_path.join("outcomes");
        let mut results = Vec::new();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(bytes) = fs::read(&path) {
                    if let Ok(o) = serde_json::from_slice::<OutcomeRecord>(&bytes) {
                        results.push(o);
                    }
                }
            }
        }
        Ok(results)
    }
}

impl GatewayConfigRepository for FileStore {
    fn load_config(&self) -> Result<LlmGatewayConfig, PersistenceError> {
        let _guard = self.lock.read().unwrap();
        let path = self.base_path.join("config").join("gateway.json");
        if !path.exists() {
            return Ok(LlmGatewayConfig::default());
        }
        let bytes = fs::read(&path)?;
        let cfg: LlmGatewayConfig = serde_json::from_slice(&bytes)?;
        Ok(cfg)
    }

    fn save_config(&self, config: &LlmGatewayConfig) -> Result<(), PersistenceError> {
        let _guard = self.lock.write().unwrap();
        let path = self.base_path.join("config").join("gateway.json");
        let bytes = serde_json::to_vec_pretty(config)?;
        self.atomic_write(&path, &bytes)
    }
}

impl cos_audit::AuditWriterPort for FileStore {
    fn record(&self, event: AuditEvent) -> Result<(), String> {
        self.append_event(&event).map_err(|e| e.to_string())
    }
}
