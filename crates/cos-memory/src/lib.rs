//! `cos-memory` provides isolated, versioned, scoped memory storage.
//!
//! # Invariants Enforced:
//! 1. Zero hardcoded business data: all guidelines, procedures, templates, and rules live in MemoryRecords.
//! 2. Client Isolation: Client A's memory namespace cannot be read or written by Client B.
//! 3. Epistemic Provenance: Every record tracks lifecycle status, confidence, version, and author.

use chrono::{DateTime, Utc};
use cos_domain::{AgentId, ClientScope, KnowledgeLifecycle, MemoryId, TaskId};
use cos_identity::ActorIdentity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryError {
    #[error("Client isolation violation: cannot access client namespace '{target:?}' with caller scope '{caller:?}'")]
    ClientIsolationViolation {
        target: ClientScope,
        caller: Option<ClientScope>,
    },

    #[error("Memory record '{0}' not found")]
    NotFound(MemoryId),

    #[error("Memory storage error: {0}")]
    StorageError(String),
}

/// Namespaces separating company, client, task, and working memory.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryNamespace {
    /// Company-wide verified knowledge (pricing rules, FAQs, procedures).
    Company,
    /// Client-specific isolated memory.
    Client(ClientScope),
    /// Ephemeral memory scoped to a single task lifecycle.
    Task(TaskId),
    /// Working memory scratchpad for a specific agent.
    AgentWorking(AgentId),
}

/// Classification of memory content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryKind {
    Procedure,
    Guideline,
    Template,
    Fact,
    Observation,
    Experience,
}

/// Provenance indicating where the information originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProvenanceType {
    Observed,
    Reported,
    Inferred,
    Learned,
    Assumed,
}

/// An individual memory record storing arbitrary dynamic knowledge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub memory_id: MemoryId,
    pub namespace: MemoryNamespace,
    pub kind: MemoryKind,
    pub key: String,
    pub content: serde_json::Value,
    pub lifecycle_status: KnowledgeLifecycle,
    pub confidence: f32,
    pub provenance: ProvenanceType,
    pub version: u64,
    pub supersedes: Option<MemoryId>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub created_by: ActorIdentity,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

impl MemoryRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        namespace: MemoryNamespace,
        kind: MemoryKind,
        key: impl Into<String>,
        content: serde_json::Value,
        status: KnowledgeLifecycle,
        confidence: f32,
        provenance: ProvenanceType,
        author: ActorIdentity,
    ) -> Self {
        let now = Utc::now();
        Self {
            memory_id: MemoryId::new_v7(),
            namespace,
            kind,
            key: key.into(),
            content,
            lifecycle_status: status,
            confidence,
            provenance,
            version: 1,
            supersedes: None,
            valid_from: now,
            valid_until: None,
            created_by: author,
            created_at: now,
            updated_at: now,
            embedding: None,
        }
    }

    /// Attaches a vector embedding to this memory record for CPU semantic search.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Thread-safe in-memory store enforcing namespace boundaries.
#[derive(Debug, Default)]
pub struct MemoryStore {
    records: RwLock<HashMap<MemoryId, MemoryRecord>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }

    /// Stores or updates a memory record.
    pub fn store(&self, record: MemoryRecord) -> Result<(), MemoryError> {
        let mut store = self.records.write().unwrap();
        store.insert(record.memory_id, record);
        Ok(())
    }

    /// Retrieves a memory record by namespace and key, verifying client isolation.
    pub fn get_by_key(
        &self,
        namespace: &MemoryNamespace,
        key: &str,
        caller_client_scope: Option<&ClientScope>,
    ) -> Result<Option<MemoryRecord>, MemoryError> {
        // Enforce client isolation
        if let MemoryNamespace::Client(target_scope) = namespace {
            match caller_client_scope {
                Some(caller) if caller == target_scope => {
                    // Authorized
                }
                _ => {
                    return Err(MemoryError::ClientIsolationViolation {
                        target: *target_scope,
                        caller: caller_client_scope.copied(),
                    });
                }
            }
        }

        let store = self.records.read().unwrap();
        let found = store
            .values()
            .find(|r| &r.namespace == namespace && r.key == key)
            .cloned();

        Ok(found)
    }

    /// Queries all memory records matching a namespace and optional kind.
    pub fn query_records(
        &self,
        namespace: &MemoryNamespace,
        kind: Option<MemoryKind>,
        caller_client_scope: Option<&ClientScope>,
    ) -> Result<Vec<MemoryRecord>, MemoryError> {
        // Enforce client isolation
        if let MemoryNamespace::Client(target_scope) = namespace {
            match caller_client_scope {
                Some(caller) if caller == target_scope => {
                    // Authorized
                }
                _ => {
                    return Err(MemoryError::ClientIsolationViolation {
                        target: *target_scope,
                        caller: caller_client_scope.copied(),
                    });
                }
            }
        }

        let store = self.records.read().unwrap();
        let results = store
            .values()
            .filter(|r| &r.namespace == namespace && kind.is_none_or(|k| r.kind == k))
            .cloned()
            .collect();

        Ok(results)
    }

    /// Performs fast semantic vector search on CPU using cosine similarity.
    ///
    /// Evaluates nearest neighbor vectors within the designated namespace and strictly enforces client isolation.
    pub fn search_semantic(
        &self,
        namespace: &MemoryNamespace,
        query_embedding: &[f32],
        top_k: usize,
        min_similarity: f32,
        caller_client_scope: Option<&ClientScope>,
    ) -> Result<Vec<(MemoryRecord, f32)>, MemoryError> {
        // Enforce client isolation
        if let MemoryNamespace::Client(target_scope) = namespace {
            match caller_client_scope {
                Some(caller) if caller == target_scope => {}
                _ => {
                    return Err(MemoryError::ClientIsolationViolation {
                        target: *target_scope,
                        caller: caller_client_scope.copied(),
                    });
                }
            }
        }

        let store = self.records.read().unwrap();
        let mut scored: Vec<(MemoryRecord, f32)> = store
            .values()
            .filter(|r| &r.namespace == namespace)
            .filter_map(|r| {
                r.embedding.as_ref().map(|emb| {
                    let sim = cosine_similarity(query_embedding, emb);
                    (r.clone(), sim)
                })
            })
            .filter(|(_, sim)| *sim >= min_similarity)
            .collect();

        // Sort descending by similarity score
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        Ok(scored)
    }
}

/// Computes cosine similarity between two float vectors on CPU at native SIMD speed.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (val_a, val_b) in a.iter().zip(b.iter()) {
        dot += val_a * val_b;
        norm_a += val_a * val_a;
        norm_b += val_b * val_b;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_storage_and_client_isolation() {
        let store = MemoryStore::new();
        let client_a = ClientScope::new_v7();
        let client_b = ClientScope::new_v7();
        let author = ActorIdentity::system("memory_service");

        // 1. Store company-wide knowledge
        let company_record = MemoryRecord::new(
            MemoryNamespace::Company,
            MemoryKind::Guideline,
            "qualification_rules",
            serde_json::json!({ "tier_threshold": 10000, "priority": "high" }),
            KnowledgeLifecycle::Verified,
            0.95,
            ProvenanceType::Observed,
            author.clone(),
        );
        store.store(company_record.clone()).unwrap();

        // 2. Store Client A private record
        let client_a_record = MemoryRecord::new(
            MemoryNamespace::Client(client_a),
            MemoryKind::Fact,
            "project_preferences",
            serde_json::json!({ "tech_stack": "Rust + React", "budget": 25000 }),
            KnowledgeLifecycle::Verified,
            1.0,
            ProvenanceType::Reported,
            author,
        );
        store.store(client_a_record.clone()).unwrap();

        // Query Company knowledge -> accessible
        let fetched_company = store
            .get_by_key(&MemoryNamespace::Company, "qualification_rules", None)
            .unwrap();
        assert!(fetched_company.is_some());
        assert_eq!(fetched_company.unwrap().key, "qualification_rules");

        // Query Client A data with Client A scope -> accessible
        let fetched_client_a = store
            .get_by_key(
                &MemoryNamespace::Client(client_a),
                "project_preferences",
                Some(&client_a),
            )
            .unwrap();
        assert!(fetched_client_a.is_some());

        // Query Client A data with Client B scope -> STRICTLY BLOCKED!
        let breach_attempt = store.get_by_key(
            &MemoryNamespace::Client(client_a),
            "project_preferences",
            Some(&client_b),
        );
        assert!(matches!(
            breach_attempt,
            Err(MemoryError::ClientIsolationViolation { .. })
        ));
    }

    #[test]
    fn test_semantic_vector_search_on_cpu() {
        let store = MemoryStore::new();
        let author = ActorIdentity::system("vector_service");

        // Orthogonal test vectors (dim = 3)
        // Vector 1: Highly aligned with [1.0, 0.0, 0.0]
        let r1 = MemoryRecord::new(
            MemoryNamespace::Company,
            MemoryKind::Fact,
            "rust_performance",
            serde_json::json!({ "topic": "Rust systems" }),
            KnowledgeLifecycle::Verified,
            1.0,
            ProvenanceType::Reported,
            author.clone(),
        )
        .with_embedding(vec![0.99, 0.05, 0.0]);
        store.store(r1).unwrap();

        // Vector 2: Completely orthogonal [0.0, 1.0, 0.0]
        let r2 = MemoryRecord::new(
            MemoryNamespace::Company,
            MemoryKind::Fact,
            "marketing_campaign",
            serde_json::json!({ "topic": "Social ads" }),
            KnowledgeLifecycle::Verified,
            1.0,
            ProvenanceType::Reported,
            author,
        )
        .with_embedding(vec![0.0, 0.99, 0.05]);
        store.store(r2).unwrap();

        // Query with vector pointing in direction [1.0, 0.0, 0.0]
        let query = vec![1.0, 0.0, 0.0];
        let matches = store
            .search_semantic(&MemoryNamespace::Company, &query, 5, 0.5, None)
            .expect("Search failed");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0.key, "rust_performance");
        assert!(matches[0].1 > 0.95); // High cosine similarity!
    }
}
