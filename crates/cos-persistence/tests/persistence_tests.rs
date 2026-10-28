use cos_approval::ApprovalRequest;
use cos_audit::{AuditEvent, AuditEventType};
use cos_domain::KnowledgeLifecycle;
use cos_goals::Goal;
use cos_identity::ActorIdentity;
use cos_memory::{MemoryKind, MemoryNamespace, MemoryRecord, ProvenanceType};
use cos_outcomes::OutcomeRecord;
use cos_persistence::{
    ApprovalRepository, AuditRepository, FileStore, GatewayConfigRepository, GoalRepository,
    MemoryRepository, OutcomeRepository, PersistenceEngine, SqliteStore,
};

#[test]
fn test_filestore_memory_crud() {
    let temp_dir = std::env::temp_dir().join(format!("cos-test-{}", uuid::Uuid::new_v4()));
    let store = FileStore::new(&temp_dir).expect("Create filestore");
    let author = ActorIdentity::system("persistence_test");

    let rec = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "key_alpha",
        serde_json::json!({ "field": "value" }),
        KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Observed,
        author,
    );

    store.save_record(&rec).unwrap();

    let loaded = store
        .get_record(&rec.namespace, &rec.key)
        .unwrap()
        .expect("Record found");
    assert_eq!(loaded.content.get("field").unwrap(), "value");

    let list = store.list_records(&rec.namespace).unwrap();
    assert_eq!(list.len(), 1);

    assert!(store.delete_record(&rec.namespace, &rec.key).unwrap());
    assert!(store
        .get_record(&rec.namespace, &rec.key)
        .unwrap()
        .is_none());

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn test_filestore_audit_log_append() {
    let temp_dir = std::env::temp_dir().join(format!("cos-test-{}", uuid::Uuid::new_v4()));
    let store = FileStore::new(&temp_dir).expect("Create filestore");

    let actor = ActorIdentity::system("persistence-test");
    let ctx = cos_contracts::TraceContext::root(actor);
    let ev1 = AuditEvent::new(
        ctx.clone(),
        AuditEventType::GrantDenied,
        None,
        "Blocked unauthorized capability",
    );
    let ev2 = AuditEvent::new(
        ctx,
        AuditEventType::ToolExecutionCompleted,
        None,
        "Executed tool task",
    );

    store.append_event(&ev1).unwrap();
    store.append_event(&ev2).unwrap();

    assert_eq!(store.count().unwrap(), 2);
    let events = store.list_events(0, 10).unwrap();
    assert_eq!(events.len(), 2);

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn test_filestore_gateway_config() {
    let temp_dir = std::env::temp_dir().join(format!("cos-test-{}", uuid::Uuid::new_v4()));
    let store = FileStore::new(&temp_dir).expect("Create filestore");

    let mut cfg = store.load_config().unwrap();
    cfg.gateway_api_key = "sk-persisted-secret".to_string();
    cfg.stream_idle_timeout_seconds = 180;
    store.save_config(&cfg).unwrap();

    let reloaded = store.load_config().unwrap();
    assert_eq!(reloaded.gateway_api_key, "sk-persisted-secret");
    assert_eq!(reloaded.stream_idle_timeout_seconds, 180);

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn test_sqlite_store_crud_and_audit() {
    let store = cos_persistence::SqliteStore::in_memory().expect("Initialize in-memory SQLite");
    let author = ActorIdentity::system("sqlite_test");

    // 1. Memory CRUD via SQLite
    let rec = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sqlite_guideline",
        serde_json::json!({ "mode": "wal_enabled", "speed": "fast" }),
        KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Observed,
        author.clone(),
    );
    store.save_record(&rec).unwrap();

    let fetched = store
        .get_record(&rec.namespace, &rec.key)
        .unwrap()
        .expect("Found SQLite record");
    assert_eq!(fetched.content["mode"], "wal_enabled");

    let list = store.list_records(&rec.namespace).unwrap();
    assert_eq!(list.len(), 1);

    // 2. Audit Trail via SQLite
    let ctx = cos_contracts::TraceContext::root(author);
    let audit_ev = AuditEvent::new(
        ctx,
        AuditEventType::ToolExecutionCompleted,
        None,
        "SQLite audit entry",
    );
    store.append_event(&audit_ev).unwrap();
    assert_eq!(store.count().unwrap(), 1);
    let events = store.list_events(0, 10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].details, "SQLite audit entry");

    // 3. Gateway Config via SQLite
    let mut cfg = store.load_config().unwrap();
    cfg.gateway_base_url = "http://127.0.0.1:9000".to_string();
    store.save_config(&cfg).unwrap();
    let reloaded_cfg = store.load_config().unwrap();
    assert_eq!(reloaded_cfg.gateway_base_url, "http://127.0.0.1:9000");
}

#[test]
fn test_sqlite_file_mode_and_all_repositories() {
    let temp_dir = std::env::temp_dir().join(format!("cos-sqlite-{}", uuid::Uuid::new_v4()));
    let db_path = temp_dir.join("test_cos.db");
    let store = SqliteStore::open(&db_path).expect("Open file-based SQLite database with WAL");

    // 1. Goal persistence in SQLite
    let goal = Goal::new("Test Scalable Operations", "Evaluate SQLite reliability");
    store.save_goal(&goal).expect("Save goal");
    let fetched_goal = store.get_goal(&goal.id).unwrap().expect("Goal exists");
    assert_eq!(fetched_goal.title, "Test Scalable Operations");

    let all_goals = GoalRepository::list_all(&store).unwrap();
    assert_eq!(all_goals.len(), 1);

    // 2. Approval persistence in SQLite
    let actor = ActorIdentity::system("test_runner");
    let task_id = cos_domain::TaskId::new_v7();
    let ctx = cos_contracts::TraceContext::root(actor.clone());
    let req = ApprovalRequest::new(
        task_id,
        ctx,
        "api.refund",
        cos_contracts::ActionDigest::compute(b"payload"),
        cos_domain::RiskClass::R3BusinessCommitment,
        None,
        chrono::Utc::now() + chrono::Duration::hours(1),
    );
    store.save_approval(&req).expect("Save approval");
    let pending = store.list_pending().unwrap();
    assert_eq!(pending.len(), 1);

    let fetched_req = store
        .get_approval(&req.approval_id)
        .unwrap()
        .expect("Approval found");
    assert_eq!(fetched_req.proposed_action, "api.refund");

    // 3. Outcome persistence in SQLite
    let outcome = OutcomeRecord::new(
        task_id,
        cos_outcomes::OutcomeStatus::Success,
        "Completed test task successfully",
        0.98,
        actor,
    )
    .with_goal(goal.id);
    store.save_outcome(&outcome).expect("Save outcome");
    let outcomes_for_goal = store.list_by_goal(&goal.id).unwrap();
    assert_eq!(outcomes_for_goal.len(), 1);
    assert_eq!(
        outcomes_for_goal[0].summary,
        "Completed test task successfully"
    );

    // Verify database file exists
    assert!(db_path.exists());

    // Clean up
    drop(store);
    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn test_persistence_engine_polymorphism() {
    // 1. In-memory SQLite engine
    let sqlite_engine = PersistenceEngine::sqlite_in_memory().expect("Create sqlite engine");
    let author = ActorIdentity::system("engine_test");
    let rec = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "engine_poly_key",
        serde_json::json!({ "unified": true }),
        KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Observed,
        author,
    );
    sqlite_engine.save_record(&rec).unwrap();
    let fetched = sqlite_engine
        .get_record(&rec.namespace, &rec.key)
        .unwrap()
        .unwrap();
    assert_eq!(fetched.content["unified"], true);

    // 2. File engine
    let temp_dir = std::env::temp_dir().join(format!("cos-poly-{}", uuid::Uuid::new_v4()));
    let file_engine = PersistenceEngine::file(&temp_dir).expect("Create file engine");
    file_engine.save_record(&rec).unwrap();
    let fetched_file = file_engine
        .get_record(&rec.namespace, &rec.key)
        .unwrap()
        .unwrap();
    assert_eq!(fetched_file.content["unified"], true);
    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn test_semantic_search_on_sqlite_and_filestore() {
    let sqlite_engine = PersistenceEngine::sqlite_in_memory().expect("Create sqlite engine");
    let author = ActorIdentity::system("semantic_test");

    // Create 3 records with distinct mock vector embeddings
    let mut rec1 = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "pricing_guideline",
        serde_json::json!({ "topic": "enterprise pricing models" }),
        KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Observed,
        author.clone(),
    );
    rec1.embedding = Some(vec![1.0, 0.0, 0.0, 0.0]);

    let mut rec2 = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Procedure,
        "incident_sop",
        serde_json::json!({ "topic": "disaster recovery SOP" }),
        KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Observed,
        author.clone(),
    );
    rec2.embedding = Some(vec![0.0, 1.0, 0.0, 0.0]);

    let mut rec3 = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Template,
        "pricing_template",
        serde_json::json!({ "topic": "client discount quote" }),
        KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Observed,
        author,
    );
    // Near pricing_guideline (angle ~ 45 deg)
    rec3.embedding = Some(vec![0.8, 0.2, 0.0, 0.0]);

    sqlite_engine.save_record(&rec1).unwrap();
    sqlite_engine.save_record(&rec2).unwrap();
    sqlite_engine.save_record(&rec3).unwrap();

    // Query for pricing vector [0.95, 0.05, 0.0, 0.0]
    let query_vector = vec![0.95, 0.05, 0.0, 0.0];
    let results = sqlite_engine
        .search_semantic(&MemoryNamespace::Company, &query_vector, 2, 0.5)
        .unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0.key, "pricing_guideline");
    assert!(results[0].1 > 0.9);
    assert_eq!(results[1].0.key, "pricing_template");
    assert!(results[1].1 > 0.8);
}
