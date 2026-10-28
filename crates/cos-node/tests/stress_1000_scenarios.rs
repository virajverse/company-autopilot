use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use chrono::{Duration, Utc};
use cos_approval::{ApprovalError, ApprovalRequest};
use cos_contracts::{ActionDigest, Capability, CommandEnvelope, TaskGrant, TraceContext};
use cos_domain::{
    ClientScope, GrantId, GrantStatus, RiskClass, SubjectId, TaskId, TenantId, TraceId,
};
use cos_goals::{ComparisonOp, Goal, GoalForest, GoalPriority, SuccessCriterion};
use cos_governance::{AuthorizationDecision, GovernanceEngine};
use cos_identity::ActorIdentity;
use cos_memory::{
    cosine_similarity, MemoryError, MemoryKind, MemoryNamespace, MemoryRecord, MemoryStore,
    ProvenanceType,
};
use cos_persistence::{AuditRepository, MemoryRepository, SqliteStore};
use cos_planning::{ExecutionPlan, PlannedTask, PlanningError};
use cos_tools::{
    FileDeliverableAdapter, NotificationToolAdapter, ToolAdapter, ToolError, ToolGateway,
};

// ============================================================================
// CLUSTER 1: ADVERSARIAL SECURITY, FRAUD & TAMPERING ATTACKS (300 SCENARIOS)
// ============================================================================
#[test]
fn test_security_adversarial_matrix_300_scenarios() {
    let start = Instant::now();
    let mut real_errors_caught = 0;
    let mut legitimate_passes = 0;

    let governance = GovernanceEngine::new();
    let now = Utc::now();
    let tenant_id = TenantId::new_v7();

    // ------------------------------------------------------------------------
    // 1.1 R4 IRREVERSIBLE CRITICAL FOUNDER BYPASS ATTACKS (60 SCENARIOS)
    // ------------------------------------------------------------------------
    for i in 0..60 {
        let task_id = TaskId::new_v7();
        let payload = format!("action_payload_r4_refund_${}", i * 1000);
        let digest = ActionDigest::compute(payload.as_bytes());

        let mut req = ApprovalRequest::new(
            task_id,
            TraceContext::root(ActorIdentity::system("test_harness")),
            format!("Authorize critical legal/financial transaction #{}", i),
            digest,
            RiskClass::R4IrreversibleCritical,
            None,
            now + Duration::hours(2),
        );

        if i % 4 != 0 {
            // Adversarial attempt: Non-founder identities try to approve R4 action
            let impostor = match i % 3 {
                0 => ActorIdentity::agent(SubjectId::new_v7(), "autonomous_lead_agent"),
                1 => ActorIdentity::executive(SubjectId::new_v7()),
                _ => ActorIdentity::system("background_cron_worker"),
            };

            let result = req.approve(impostor, "Self-authorized critical refund", now);
            assert_eq!(
                result,
                Err(ApprovalError::FounderRequired),
                "Scenario 1.1.#{} failed: Non-founder MUST be rejected with FounderRequired",
                i
            );
            real_errors_caught += 1;
        } else {
            // Legitimate: Genuine Founder signs off
            let founder = ActorIdentity::founder(SubjectId::new_v7(), None);
            let result = req.approve(founder, "Founder certified executive signoff", now);
            assert!(
                result.is_ok(),
                "Scenario 1.1.#{} failed: Genuine Founder must succeed",
                i
            );
            legitimate_passes += 1;
        }
    }

    // ------------------------------------------------------------------------
    // 1.2 CRYPTOGRAPHIC DIGEST TAMPERING ATTACKS (60 SCENARIOS)
    // ------------------------------------------------------------------------
    for i in 0..60 {
        let original_payload = format!("transfer_funds_amount_{}_recipient_corp", i * 500);
        let valid_digest = ActionDigest::compute(original_payload.as_bytes());

        let grant = TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id: TaskId::new_v7(),
            agent_id: cos_domain::AgentId::new_v7(),
            subject_id: SubjectId::new_v7(),
            role: "finance_agent".to_string(),
            allowed_capabilities: vec![Capability::new("network_http")],
            client_scope: None,
            project_scope: None,
            tenant_scope: tenant_id,
            risk_class: RiskClass::R1ReversibleLowImpact,
            approval_reference: None,
            action_digest: valid_digest,
            policy_version: 1,
            grant_status: GrantStatus::Active,
            issued_at: now,
            expires_at: now + Duration::minutes(30),
            max_calls: 5,
            issued_by: ActorIdentity::system("executive"),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        };

        if i % 5 != 0 {
            // Hostile: Payload tampered in-flight (modified recipient, injected SQL, changed amounts)
            let tampered_payload = format!("{}_injected_attacker_account_hacked", original_payload);
            let tampered_digest = ActionDigest::compute(tampered_payload.as_bytes());

            let decision = governance.authorize_tool_execution(
                &grant,
                &Capability::new("network_http"),
                &tampered_digest,
                None,
                now,
                None,
            );

            match decision {
                AuthorizationDecision::Denied { reason } => {
                    assert!(
                        reason.contains("Action digest mismatch"),
                        "Scenario 1.2.#{} failed: Must deny with digest mismatch reason: {}",
                        i,
                        reason
                    );
                    real_errors_caught += 1;
                }
                other => {
                    panic!(
                        "Scenario 1.2.#{} failed: Tampered digest passed with {:?}",
                        i, other
                    )
                }
            }
        } else {
            // Legitimate: Exact untampered digest
            let decision = governance.authorize_tool_execution(
                &grant,
                &Capability::new("network_http"),
                &valid_digest,
                None,
                now,
                None,
            );
            assert!(
                matches!(decision, AuthorizationDecision::Allowed),
                "Scenario 1.2.#{} failed: Valid digest must be allowed",
                i
            );
            legitimate_passes += 1;
        }
    }

    // ------------------------------------------------------------------------
    // 1.3 REPLAY ATTACKS & DOUBLE-SPENDING DEFENSE (60 SCENARIOS)
    // ------------------------------------------------------------------------
    for i in 0..60 {
        let task_id = TaskId::new_v7();
        let digest = ActionDigest::compute(format!("replay_action_{}", i).as_bytes());
        let mut req = ApprovalRequest::new(
            task_id,
            TraceContext::root(ActorIdentity::system("test_harness")),
            format!("Replay test approval #{}", i),
            digest,
            RiskClass::R3BusinessCommitment,
            None,
            now + Duration::hours(1),
        );

        let founder = ActorIdentity::founder(SubjectId::new_v7(), None);
        req.approve(founder, "Approved for single execution", now)
            .unwrap();

        // 1st consumption: Legitimate
        assert_eq!(req.consume(), Ok(()));
        legitimate_passes += 1;

        // 2nd consumption: Malicious replay attempt
        let replay_result = req.consume();
        assert_eq!(
            replay_result,
            Err(ApprovalError::AlreadyConsumed),
            "Scenario 1.3.#{} failed: Replay attack MUST be rejected with AlreadyConsumed",
            i
        );
        real_errors_caught += 1;
    }

    // ------------------------------------------------------------------------
    // 1.4 MULTI-TENANT CROSS-CLIENT DATA LEAK ATTACKS (60 SCENARIOS)
    // ------------------------------------------------------------------------
    let client_pool: Vec<ClientScope> = (0..60).map(|_| ClientScope::new_v7()).collect();

    for (i, &grant_scope) in client_pool.iter().enumerate().take(60) {
        let grant = TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id: TaskId::new_v7(),
            agent_id: cos_domain::AgentId::new_v7(),
            subject_id: SubjectId::new_v7(),
            role: "crm_agent".to_string(),
            allowed_capabilities: vec![Capability::new("database_read")],
            client_scope: Some(grant_scope),
            project_scope: None,
            tenant_scope: tenant_id,
            risk_class: RiskClass::R0InternalRead,
            approval_reference: None,
            action_digest: ActionDigest::compute(b"crm_lookup"),
            policy_version: 1,
            grant_status: GrantStatus::Active,
            issued_at: now,
            expires_at: now + Duration::minutes(15),
            max_calls: 10,
            issued_by: ActorIdentity::system("executive"),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        };

        if i % 3 != 0 {
            // Adversarial attempt: Target client scope does not match grant client scope
            let attacker_target_scope = ClientScope::new_v7();
            let decision = governance.authorize_tool_execution(
                &grant,
                &Capability::new("database_read"),
                &grant.action_digest,
                Some(&attacker_target_scope),
                now,
                None,
            );

            match decision {
                AuthorizationDecision::Denied { reason } => {
                    assert!(
                        reason.contains("Client scope isolation violation"),
                        "Scenario 1.4.#{} failed: Expected client scope mismatch, got: {}",
                        i,
                        reason
                    );
                    real_errors_caught += 1;
                }
                other => panic!(
                    "Scenario 1.4.#{} failed: Cross-client breach allowed with {:?}",
                    i, other
                ),
            }
        } else {
            // Legitimate: Target scope matches grant scope
            let decision = governance.authorize_tool_execution(
                &grant,
                &Capability::new("database_read"),
                &grant.action_digest,
                Some(&grant_scope),
                now,
                None,
            );
            assert!(
                matches!(decision, AuthorizationDecision::Allowed),
                "Scenario 1.4.#{} failed: Matching client scope must be allowed",
                i
            );
            legitimate_passes += 1;
        }
    }

    // ------------------------------------------------------------------------
    // 1.5 EXPIRED GRANTS & TEMPORAL BOUNDARY VIOLATIONS (60 SCENARIOS)
    // ------------------------------------------------------------------------
    for i in 0..60 {
        let is_expired = i % 2 == 0;
        let grant_expires_at = if is_expired {
            now - Duration::seconds(10 + i as i64) // in the past
        } else {
            now + Duration::minutes(10 + i as i64) // in the future
        };

        let grant = TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id: TaskId::new_v7(),
            agent_id: cos_domain::AgentId::new_v7(),
            subject_id: SubjectId::new_v7(),
            role: "ops_agent".to_string(),
            allowed_capabilities: vec![Capability::new("send_notification")],
            client_scope: None,
            project_scope: None,
            tenant_scope: tenant_id,
            risk_class: RiskClass::R1ReversibleLowImpact,
            approval_reference: None,
            action_digest: ActionDigest::compute(b"notify_payload"),
            policy_version: 1,
            grant_status: GrantStatus::Active,
            issued_at: now - Duration::minutes(20),
            expires_at: grant_expires_at,
            max_calls: 1,
            issued_by: ActorIdentity::system("executive"),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        };

        let decision = governance.authorize_tool_execution(
            &grant,
            &Capability::new("send_notification"),
            &grant.action_digest,
            None,
            now,
            None,
        );

        if is_expired {
            match decision {
                AuthorizationDecision::Denied { reason } => {
                    assert!(
                        reason.contains("expired"),
                        "Scenario 1.5.#{} failed: Must reject expired grant, got: {}",
                        i,
                        reason
                    );
                    real_errors_caught += 1;
                }
                other => panic!(
                    "Scenario 1.5.#{} failed: Expired grant allowed: {:?}",
                    i, other
                ),
            }
        } else {
            assert!(
                matches!(decision, AuthorizationDecision::Allowed),
                "Scenario 1.5.#{} failed: Active grant must be allowed",
                i
            );
            legitimate_passes += 1;
        }
    }

    let elapsed = start.elapsed();
    println!(
        "\n[CLUSTER 1 COMPLETE] 360 Security/Adversarial Scenarios Tested in {:.2?}! Real Attacks Blocked: {}, Legitimate Passes: {}",
        elapsed, real_errors_caught, legitimate_passes
    );
    assert_eq!(real_errors_caught + legitimate_passes, 360);
}

// ============================================================================
// CLUSTER 2: CONCURRENCY, THREAD RACES & HEAVY SQLITE WAL STRESS (250 SCENARIOS)
// ============================================================================
#[test]
fn test_concurrency_and_race_matrix_250_scenarios() {
    let start = Instant::now();
    let temp_dir = std::env::temp_dir().join(format!("cos-stress-wal-{}", uuid::Uuid::new_v4()));
    let db_path = temp_dir.join("concurrency_stress.db");

    // ------------------------------------------------------------------------
    // 2.1 MULTI-THREADED SQLITE WAL MODE CONCURRENCY (100 CONCURRENT OS THREADS)
    // ------------------------------------------------------------------------
    let store = Arc::new(SqliteStore::open(&db_path).expect("Open SQLite WAL mode store"));
    let mut handles = Vec::new();

    for thread_idx in 0..100 {
        let store_clone = Arc::clone(&store);
        let handle = thread::spawn(move || {
            let author = ActorIdentity::system("worker_thread");
            let context = TraceContext::root(author.clone());

            // 1. Append audit event
            let ev = cos_audit::AuditEvent::new(
                context,
                cos_audit::AuditEventType::ToolExecutionCompleted,
                None,
                format!("Concurrent audit entry from worker {}", thread_idx),
            );
            store_clone
                .append_event(&ev)
                .expect("WAL concurrent audit write");

            // 2. Insert distinct memory record
            let rec = MemoryRecord::new(
                MemoryNamespace::Company,
                MemoryKind::Fact,
                format!("key_worker_{}", thread_idx),
                serde_json::json!({ "worker": thread_idx, "wal_verified": true }),
                cos_domain::KnowledgeLifecycle::Verified,
                1.0,
                ProvenanceType::Observed,
                author,
            );
            store_clone
                .save_record(&rec)
                .expect("WAL concurrent memory write");
        });
        handles.push(handle);
    }

    for h in handles {
        h.join()
            .expect("Worker thread panicked under concurrency load");
    }

    // Verify exactly 100 audit entries and 100 memory records were stored without data corruption
    assert_eq!(store.count().expect("Count audit events"), 100);
    let records = store
        .list_records(&MemoryNamespace::Company)
        .expect("List memory records");
    assert_eq!(records.len(), 100);

    // ------------------------------------------------------------------------
    // 2.2 CONCURRENT DOUBLE-SPEND RACES ON APPROVALS (100 RACES = 50 PAIRS)
    // ------------------------------------------------------------------------
    let mut double_spend_rejections = 0;
    let mut legitimate_consumptions = 0;

    for race_idx in 0..50 {
        let task_id = TaskId::new_v7();
        let digest = ActionDigest::compute(format!("race_payload_{}", race_idx).as_bytes());
        let mut req = ApprovalRequest::new(
            task_id,
            TraceContext::root(ActorIdentity::system("test_harness")),
            format!("Race approval #{}", race_idx),
            digest,
            RiskClass::R3BusinessCommitment,
            None,
            Utc::now() + Duration::hours(1),
        );
        let founder = ActorIdentity::founder(SubjectId::new_v7(), None);
        req.approve(founder, "Authorized", Utc::now()).unwrap();

        let shared_req = Arc::new(Mutex::new(req));
        let req_a = Arc::clone(&shared_req);
        let req_b = Arc::clone(&shared_req);

        let t1 = thread::spawn(move || req_a.lock().unwrap().consume());
        let t2 = thread::spawn(move || req_b.lock().unwrap().consume());

        let res1 = t1.join().unwrap();
        let res2 = t2.join().unwrap();

        let mut successes = 0;
        let mut rejections = 0;

        match res1 {
            Ok(()) => successes += 1,
            Err(ApprovalError::AlreadyConsumed) => rejections += 1,
            other => panic!("Unexpected race result: {:?}", other),
        }

        match res2 {
            Ok(()) => successes += 1,
            Err(ApprovalError::AlreadyConsumed) => rejections += 1,
            other => panic!("Unexpected race result: {:?}", other),
        }

        assert_eq!(
            successes, 1,
            "Race #{} failed: Exactly ONE thread MUST succeed in consuming",
            race_idx
        );
        assert_eq!(
            rejections, 1,
            "Race #{} failed: Exactly ONE thread MUST be rejected with AlreadyConsumed",
            race_idx
        );

        legitimate_consumptions += successes;
        double_spend_rejections += rejections;
    }

    // ------------------------------------------------------------------------
    // 2.3 TOOL GATEWAY IDEMPOTENCY STORM (50 SCENARIOS: 40 SAFE REPLAYS, 10 CONFLICTS)
    // ------------------------------------------------------------------------
    let audit_writer = Arc::new(SqliteStore::in_memory().unwrap());
    let gateway = ToolGateway::new(audit_writer.clone());
    gateway.register_tool(Arc::new(NotificationToolAdapter::new()));

    let tenant_id = TenantId::new_v7();
    let mut idempotency_replays = 0;
    let mut idempotency_conflicts = 0;

    for storm_idx in 0..50 {
        let idempotency_key = format!("storm_key_{}", storm_idx);
        let action_payload = serde_json::json!({
            "recipient": "user@enterprise.com",
            "message": format!("Notification payload {}", storm_idx)
        });
        let action_digest = ActionDigest::compute(action_payload.to_string().as_bytes());

        let grant = TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id: TaskId::new_v7(),
            agent_id: cos_domain::AgentId::new_v7(),
            subject_id: SubjectId::new_v7(),
            role: "notifier".to_string(),
            allowed_capabilities: vec![Capability::new("send_notification")],
            client_scope: None,
            project_scope: None,
            tenant_scope: tenant_id,
            risk_class: RiskClass::R1ReversibleLowImpact,
            approval_reference: None,
            action_digest,
            policy_version: 1,
            grant_status: GrantStatus::Active,
            issued_at: Utc::now(),
            expires_at: Utc::now() + Duration::minutes(10),
            max_calls: 10,
            issued_by: ActorIdentity::system("executive"),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        };

        let cmd = CommandEnvelope {
            command_id: uuid::Uuid::now_v7(),
            command_type: "send_notification".to_string(),
            context: TraceContext::root(ActorIdentity::system("test")),
            task_id: Some(grant.task_id),
            goal_id: None,
            client_scope: None,
            project_scope: None,
            payload: action_payload.clone(),
            expected_version: 1,
            capability_grant: Some(grant.clone()),
            approval_reference: None,
            action_digest,
            idempotency_key: idempotency_key.clone(),
            created_at: Utc::now(),
        };

        // 1. Initial execution
        let first_run = gateway.execute_command(&cmd, &grant, "notification_sender", None, None);
        if let Err(ref e) = first_run {
            panic!("Initial tool execution failed with error: {:?}", e);
        }
        assert!(first_run.is_ok(), "Initial tool execution must succeed");

        if storm_idx < 40 {
            // 2. Safe replay with IDENTICAL digest: Must return cached result cleanly
            let replay_run =
                gateway.execute_command(&cmd, &grant, "notification_sender", None, None);
            assert!(replay_run.is_ok(), "Idempotent replay must succeed");
            assert_eq!(
                replay_run.unwrap().execution_id,
                first_run.unwrap().execution_id
            );
            idempotency_replays += 1;
        } else {
            // 3. Adversarial conflict: Re-using the same idempotency key with a DIFFERENT payload
            let altered_payload = serde_json::json!({
                "recipient": "hacked@evil.com",
                "message": "Forged payload under existing key"
            });
            let altered_digest = ActionDigest::compute(altered_payload.to_string().as_bytes());

            let mut altered_grant = grant.clone();
            altered_grant.action_digest = altered_digest;

            let mut altered_cmd = cmd.clone();
            altered_cmd.payload = altered_payload;
            altered_cmd.action_digest = altered_digest;
            altered_cmd.capability_grant = Some(altered_grant.clone());

            let conflict_run = gateway.execute_command(
                &altered_cmd,
                &altered_grant,
                "notification_sender",
                None,
                None,
            );

            assert_eq!(
                conflict_run,
                Err(ToolError::IdempotencyConflict {
                    key: idempotency_key
                }),
                "Conflicting payload with reused idempotency key MUST be rejected with IdempotencyConflict"
            );
            idempotency_conflicts += 1;
        }
    }

    drop(store);
    let _ = std::fs::remove_dir_all(temp_dir);

    let elapsed = start.elapsed();
    println!(
        "\n[CLUSTER 2 COMPLETE] 250 Concurrency/Stress Scenarios in {:.2?}! WAL Writes: 100, Anti-Replay Races Defended: {}, Idempotency Replays: {}, Idempotency Conflicts Blocked: {}",
        elapsed, double_spend_rejections, idempotency_replays, idempotency_conflicts
    );
    assert_eq!(
        100 + legitimate_consumptions
            + double_spend_rejections
            + idempotency_replays
            + idempotency_conflicts,
        250
    );
}

// ============================================================================
// CLUSTER 3: CPU SEMANTIC VECTOR MATH & MEMORY FUZZING (250 SCENARIOS)
// ============================================================================
#[test]
fn test_cpu_semantic_vector_fuzzing_250_scenarios() {
    let start = Instant::now();

    // ------------------------------------------------------------------------
    // 3.1 VECTOR MATH BOUNDARY PERMUTATIONS (100 SCENARIOS)
    // ------------------------------------------------------------------------
    for i in 0..100 {
        match i % 5 {
            0 => {
                // Zero vector: Cosine similarity MUST return 0.0 without NaN or divide-by-zero panic
                let zero_v = vec![0.0; 16];
                let arbitrary = vec![1.0; 16];
                let sim = cosine_similarity(&zero_v, &arbitrary);
                assert_eq!(sim, 0.0, "Zero vector cosine must safely evaluate to 0.0");
            }
            1 => {
                // Exactly identical vectors: Cosine similarity MUST equal 1.0 (within float tolerance)
                let v = vec![0.57735026, 0.57735026, 0.57735026];
                let sim = cosine_similarity(&v, &v);
                assert!(
                    (sim - 1.0).abs() < 1e-4,
                    "Identical vector cosine must be ~1.0: {}",
                    sim
                );
            }
            2 => {
                // Orthogonal vectors: Cosine similarity MUST equal 0.0
                let v1 = vec![1.0, 0.0, 0.0, 0.0];
                let v2 = vec![0.0, 1.0, 0.0, 0.0];
                let sim = cosine_similarity(&v1, &v2);
                assert!(
                    sim.abs() < 1e-5,
                    "Orthogonal vector cosine must be 0.0: {}",
                    sim
                );
            }
            3 => {
                // Opposite / Antipodal vectors: Cosine similarity MUST equal -1.0
                let v1 = vec![0.5, 0.5, 0.5, 0.5];
                let v2 = vec![-0.5, -0.5, -0.5, -0.5];
                let sim = cosine_similarity(&v1, &v2);
                assert!(
                    (sim - (-1.0)).abs() < 1e-4,
                    "Antipodal vector cosine must be -1.0: {}",
                    sim
                );
            }
            _ => {
                // High-dimensional vector (384-dimensional): Verify SIMD stability
                let mut v1 = Vec::with_capacity(384);
                let mut v2 = Vec::with_capacity(384);
                for idx in 0..384 {
                    v1.push((idx as f32).sin());
                    v2.push((idx as f32).cos());
                }
                let sim = cosine_similarity(&v1, &v2);
                assert!(
                    !sim.is_nan(),
                    "High-dimensional float vector must not produce NaN"
                );
                assert!(
                    (-1.0..=1.0).contains(&sim),
                    "Cosine similarity must be clamped in [-1.0, 1.0]"
                );
            }
        }
    }

    // ------------------------------------------------------------------------
    // 3.2 SEMANTIC VECTOR TOP-K RETRIEVAL ACROSS 1,000 MEMORY RECORDS (150 SCENARIOS)
    // ------------------------------------------------------------------------
    let mem_store = MemoryStore::new();
    let author = ActorIdentity::system("vector_seeder");
    let client_scopes: Vec<ClientScope> = (0..10).map(|_| ClientScope::new_v7()).collect();

    // Seed 1,000 records in memory with distinct embeddings & client scopes
    for i in 0..1000 {
        let client_scope = client_scopes[i % 10];

        let mut embed = vec![0.0; 8];
        let category_idx = i % 8;
        embed[category_idx] = 1.0; // One-hot directional embedding

        let rec = MemoryRecord::new(
            MemoryNamespace::Client(client_scope),
            MemoryKind::Fact,
            format!("client_fact_{}", i),
            serde_json::json!({ "record_num": i, "category": category_idx }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            ProvenanceType::Observed,
            author.clone(),
        )
        .with_embedding(embed);

        mem_store.store(rec).unwrap();
    }

    // Perform 150 search scenarios:
    // 100 legitimate queries with matching client scope
    // 50 adversarial queries attempting cross-client data breaches
    let mut legitimate_searches = 0;
    let mut isolation_violations_blocked = 0;

    for search_idx in 0..150 {
        let target_category = search_idx % 8;
        let mut query_vec = vec![0.0; 8];
        query_vec[target_category] = 1.0;

        let target_scope = client_scopes[search_idx % 10];

        if search_idx < 100 {
            // Legitimate: Search within authorized client scope
            let results = mem_store
                .search_semantic(
                    &MemoryNamespace::Client(target_scope),
                    &query_vec,
                    5,
                    0.8,
                    Some(&target_scope),
                )
                .expect("Authorized semantic search must succeed");

            for (_rec, score) in results {
                assert!(
                    score >= 0.8,
                    "Returned match below min_similarity threshold: {}",
                    score
                );
            }
            legitimate_searches += 1;
        } else {
            // Adversarial attempt: Caller with attacker_client tries to query victim's client namespace
            let attacker_scope = ClientScope::new_v7();
            let res = mem_store.search_semantic(
                &MemoryNamespace::Client(target_scope),
                &query_vec,
                5,
                0.8,
                Some(&attacker_scope),
            );

            assert!(
                matches!(res, Err(MemoryError::ClientIsolationViolation { .. })),
                "Scenario 3.2.#{} failed: Cross-client semantic query MUST return ClientIsolationViolation",
                search_idx
            );
            isolation_violations_blocked += 1;
        }
    }

    let elapsed = start.elapsed();
    println!(
        "\n[CLUSTER 3 COMPLETE] 250 Vector Math & Semantic Search Scenarios in {:.2?}! 1,000 Records Tested. Authorized Searches: {}, Cross-Client Breaches Blocked: {}",
        elapsed, legitimate_searches, isolation_violations_blocked
    );
    assert_eq!(
        100 + legitimate_searches + isolation_violations_blocked,
        250
    );
}

// ============================================================================
// CLUSTER 4: GOAL FOREST, DAG CYCLE INJECTIONS & PROGRESS CHAOS (200 SCENARIOS)
// ============================================================================
#[test]
fn test_goal_dag_cycle_chaos_200_scenarios() {
    let start = Instant::now();
    let mut cycles_detected = 0;
    let mut valid_plans_passed = 0;

    // ------------------------------------------------------------------------
    // 4.1 DAG CYCLE INJECTION ATTACKS (KAHN'S ALGORITHM VALIDATION) (100 SCENARIOS)
    // ------------------------------------------------------------------------
    for i in 0..100 {
        let goal_id = cos_goals::GoalId::new_v7();

        let t1 = PlannedTask::new(
            goal_id,
            "Step 1",
            "Initial task",
            "ops",
            RiskClass::R0InternalRead,
        );
        let t2 = PlannedTask::new(
            goal_id,
            "Step 2",
            "Middle task",
            "ops",
            RiskClass::R0InternalRead,
        )
        .with_prerequisite(t1.id);

        if i % 2 == 0 {
            // Adversarial: Inject circular dependency (t1 requires t3, t3 requires t2, t2 requires t1)
            let t3 = PlannedTask::new(
                goal_id,
                "Step 3",
                "Circular task",
                "ops",
                RiskClass::R0InternalRead,
            )
            .with_prerequisite(t2.id);

            // Close the cycle: t1 now requires t3
            let mut t1_cyclic = t1.clone();
            t1_cyclic.prerequisite_task_ids.push(t3.id);

            let plan_res = ExecutionPlan::new(
                goal_id,
                format!("Cyclic Plan #{}", i),
                vec![t1_cyclic, t2, t3],
            );
            assert_eq!(
                plan_res,
                Err(PlanningError::CyclicDependency),
                "Scenario 4.1.#{} failed: Cyclic dependency MUST be detected and rejected",
                i
            );
            cycles_detected += 1;
        } else {
            // Legitimate: Valid acyclic DAG (t1 -> t2 -> t3)
            let t3 = PlannedTask::new(
                goal_id,
                "Step 3",
                "Final task",
                "ops",
                RiskClass::R0InternalRead,
            )
            .with_prerequisite(t2.id);

            let plan_res =
                ExecutionPlan::new(goal_id, format!("Valid Plan #{}", i), vec![t1, t2, t3]);
            assert!(
                plan_res.is_ok(),
                "Scenario 4.1.#{} failed: Valid DAG plan must be accepted",
                i
            );
            valid_plans_passed += 1;
        }
    }

    // ------------------------------------------------------------------------
    // 4.2 HIERARCHICAL GOAL PROGRESS & STATUS PROPAGATION (100 SCENARIOS)
    // ------------------------------------------------------------------------
    let mut progress_updates = 0;
    for i in 0..100 {
        let mut forest = GoalForest::new();

        // Root Goal
        let root = Goal::new("Root Operation", "Enterprise Autonomous Execution")
            .with_priority(GoalPriority::Critical);
        let root_id = forest.add_goal(root).expect("Add root");

        // Subgoal 1
        let sub1 = Goal::new("Subgoal 1", "Deliverable milestones")
            .with_parent(root_id)
            .with_criterion(SuccessCriterion::new(
                "tasks_done",
                10.0,
                ComparisonOp::GreaterThanOrEqual,
                None,
            ));
        let sub1_id = forest.add_goal(sub1).expect("Add sub1");

        // Subgoal 2
        let sub2 = Goal::new("Subgoal 2", "Quality assurance")
            .with_parent(root_id)
            .with_criterion(SuccessCriterion::new(
                "tests_passed",
                50.0,
                ComparisonOp::GreaterThanOrEqual,
                None,
            ));
        let sub2_id = forest.add_goal(sub2).expect("Add sub2");

        // Update sub1 progress: 5.0 out of 10.0 = 50%
        forest
            .update_criterion(&sub1_id, "tasks_done", 5.0)
            .expect("Update criterion");

        // Update sub2 progress: 50.0 out of 50.0 = 100%
        forest
            .update_criterion(&sub2_id, "tests_passed", 50.0)
            .expect("Update criterion");

        // Auto-propagated parent progress: (50% + 100%) / 2 = 75%
        let parent_progress = forest.get_goal(&root_id).unwrap().progress_percent;
        assert!(
            (parent_progress - 75.0).abs() < 1e-2,
            "Scenario 4.2.#{} failed: Aggregate progress must equal 75.0%: got {}",
            i,
            parent_progress
        );
        progress_updates += 1;
    }

    let elapsed = start.elapsed();
    println!(
        "\n[CLUSTER 4 COMPLETE] 200 Goal & Planning Scenarios in {:.2?}! Cycles Detected & Rejected: {}, Valid DAGs Passed: {}, Hierarchy Updates: {}",
        elapsed, cycles_detected, valid_plans_passed, progress_updates
    );
    assert_eq!(cycles_detected + valid_plans_passed + progress_updates, 200);
}

// ============================================================================
// CLUSTER 5: EXECUTIVE AUTONOMOUS FLOW & TOOL PATH TRAVERSAL (200 SCENARIOS)
// ============================================================================
#[test]
fn test_executive_and_tools_flow_200_scenarios() {
    let start = Instant::now();
    let temp_dir = std::env::temp_dir().join(format!("cos-deliverables-{}", uuid::Uuid::new_v4()));
    let adapter = FileDeliverableAdapter::new(&temp_dir);

    let mut path_traversals_blocked = 0;
    let mut legitimate_writes = 0;

    // ------------------------------------------------------------------------
    // 5.1 PATH TRAVERSAL INJECTION ATTACKS ON DELIVERABLES (100 SCENARIOS)
    // ------------------------------------------------------------------------
    for i in 0..100 {
        if i % 2 == 0 {
            // Adversarial path traversal attempt
            let malicious_file_name = match i % 4 {
                0 => "../../etc/passwd",
                1 => "..\\..\\Windows\\System32\\cmd.exe",
                2 => "/absolute/root/override.json",
                _ => "subfolder/../../../secrets.env",
            };

            let payload = serde_json::json!({
                "file_name": malicious_file_name,
                "content": "malicious payload overwrite attempt"
            });

            let res = adapter.execute(&payload);
            assert!(
                res.is_err(),
                "Scenario 5.1.#{} failed: Path traversal MUST return error for '{}'",
                i,
                malicious_file_name
            );
            path_traversals_blocked += 1;
        } else {
            // Legitimate safe filename
            let safe_file = format!("client_deliverable_report_{}.md", i);
            let payload = serde_json::json!({
                "file_name": safe_file,
                "content": "# Finalized Proposal\nScope and timeline deliverables verified."
            });

            let res = adapter.execute(&payload);
            assert!(
                res.is_ok(),
                "Scenario 5.1.#{} failed: Valid deliverable write must succeed",
                i
            );
            legitimate_writes += 1;
        }
    }

    // ------------------------------------------------------------------------
    // 5.2 EXECUTIVE ROLE DISPATCH & UNKNOWN ROLE ERRORS (100 SCENARIOS)
    // ------------------------------------------------------------------------
    let mem_store = Arc::new(MemoryStore::new());
    let audit_store = Arc::new(cos_persistence::SqliteStore::in_memory().unwrap());
    let tool_gw = Arc::new(cos_tools::ToolGateway::new(audit_store.clone()));

    let executive =
        cos_executive::CompanyExecutive::new(TenantId::new_v7(), audit_store, mem_store, tool_gw);

    // Register 1 legitimate role: "sales_agent"
    executive.register_agent(cos_agents::DynamicAgent::new(
        "sales_agent",
        "Qualify incoming leads",
        vec![Capability::new("send_notification")],
    ));

    let mut unknown_roles_rejected = 0;
    let mut legitimate_tasks_processed = 0;

    for i in 0..100 {
        let is_unknown = i % 2 == 0;
        let role = if is_unknown {
            format!("unregistered_hacker_role_{}", i)
        } else {
            "sales_agent".to_string()
        };

        let params = cos_executive::ProcessTaskParams {
            task_title: format!("Execute task #{}", i),
            target_role: role.clone(),
            task_payload: serde_json::json!({ "inquiry_id": i }),
            memory_guideline_key: None,
            target_tool: "notification_sender".to_string(),
            required_capability: Capability::new("send_notification"),
            risk_class: RiskClass::R1ReversibleLowImpact,
            client_scope: None,
            custom_grant_duration_minutes: None,
            custom_approval_duration_hours: None,
            custom_max_calls: None,
        };

        let result = executive.process_generic_task(params);
        if is_unknown {
            assert!(
                matches!(result, Err(cos_executive::ExecutiveError::UnknownRole(_))),
                "Scenario 5.2.#{} failed: Unregistered role must return UnknownRole error",
                i
            );
            unknown_roles_rejected += 1;
        } else {
            assert!(
                result.is_ok(),
                "Scenario 5.2.#{} failed: Registered sales_agent must process successfully",
                i
            );
            legitimate_tasks_processed += 1;
        }
    }

    let _ = std::fs::remove_dir_all(temp_dir);

    let elapsed = start.elapsed();
    println!(
        "\n[CLUSTER 5 COMPLETE] 200 Executive & Tool Scenarios in {:.2?}! Path Traversals Blocked: {}, Deliverables Written: {}, Unknown Roles Rejected: {}, Valid Tasks: {}",
        elapsed, path_traversals_blocked, legitimate_writes, unknown_roles_rejected, legitimate_tasks_processed
    );
    assert_eq!(
        path_traversals_blocked
            + legitimate_writes
            + unknown_roles_rejected
            + legitimate_tasks_processed,
        200
    );
}
