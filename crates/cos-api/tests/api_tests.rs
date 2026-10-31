use cos_api::{build_app_router, AppState};
use cos_approval::ApprovalRequest;
use cos_domain::RiskClass;
use cos_persistence::{ApprovalRepository, FileStore, MemoryRepository};
use std::sync::Arc;

async fn spawn_test_server() -> (String, Arc<FileStore>) {
    let temp_dir = std::env::temp_dir().join(format!("cos-api-test-{}", uuid::Uuid::new_v4()));
    let store = Arc::new(FileStore::new(&temp_dir).expect("Create filestore"));

    let state = AppState::from_file_store(store.clone());
    let router = build_app_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://{}", addr), store)
}

#[tokio::test]
async fn test_health_endpoint() {
    let (base_url, _store) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{}/api/v1/health", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body.get("status").unwrap(), "healthy");
}

#[tokio::test]
async fn test_gateway_config_api_and_test_connection() {
    let (base_url, _store) = spawn_test_server().await;
    let client = reqwest::Client::new();

    // 1. Get default config (masked)
    let res = client
        .get(format!("{}/api/v1/gateway/config", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let cfg: serde_json::Value = res.json().await.unwrap();
    assert_eq!(cfg.get("stream_idle_timeout_seconds").unwrap(), 300);

    // 2. Test connection
    let res = client
        .post(format!("{}/api/v1/gateway/test-connection", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let test_res: serde_json::Value = res.json().await.unwrap();
    assert!(test_res.get("message").is_some());

    // 3. Update gateway config
    let mut update_payload = cfg.clone();
    update_payload["gateway_api_key"] = serde_json::json!("sk-live-updated-secret-12345");
    update_payload["models"]["model_list"] = serde_json::json!([
        {
            "model_id": "claude-sonnet-4-5",
            "display_name": "claude-sonnet-4-5",
            "offer_1m_variant": true,
            "only_1m_option": false,
            "is_default": true
        }
    ]);

    let res = client
        .put(format!("{}/api/v1/gateway/config", base_url))
        .json(&update_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // 4. Verify updated config is masked
    let res = client
        .get(format!("{}/api/v1/gateway/config", base_url))
        .send()
        .await
        .unwrap();
    let reloaded: serde_json::Value = res.json().await.unwrap();
    let masked_key = reloaded.get("gateway_api_key").unwrap().as_str().unwrap();
    assert!(masked_key.contains("••••"));
    assert_ne!(masked_key, "sk-live-updated-secret-12345");
}

#[tokio::test]
async fn test_approvals_workflow_api() {
    let (base_url, store) = spawn_test_server().await;
    let client = reqwest::Client::new();

    // Seed a pending approval in store
    let task_id = cos_domain::TaskId::new_v7();
    let actor = cos_identity::ActorIdentity::system("api-test");
    let ctx = cos_contracts::TraceContext::root(actor);
    let digest = cos_contracts::ActionDigest::compute(b"financial_wire");
    let expires = chrono::Utc::now() + chrono::Duration::hours(24);

    let req = ApprovalRequest::new(
        task_id,
        ctx,
        "Tool invocation: initiate_financial_wire",
        digest,
        RiskClass::R3BusinessCommitment,
        None,
        expires,
    );
    let approval_id = req.approval_id;
    store.save_approval(&req).unwrap();

    // 1. List pending
    let res = client
        .get(format!("{}/api/v1/approvals", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let list: Vec<serde_json::Value> = res.json().await.unwrap();
    assert_eq!(list.len(), 1);

    // 2. Decide approval -> Approve
    let res = client
        .post(format!(
            "{}/api/v1/approvals/{}/decide",
            base_url, approval_id.0
        ))
        .json(&serde_json::json!({
            "decision": "approve",
            "actor_id": "founder-alice"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // 3. Pending list is now empty
    let res = client
        .get(format!("{}/api/v1/approvals", base_url))
        .send()
        .await
        .unwrap();
    let list_after: Vec<serde_json::Value> = res.json().await.unwrap();
    assert_eq!(list_after.len(), 0);

    // 4. Verify audit trail recorded ApprovalDecided event
    let res = client
        .get(format!("{}/api/v1/audit", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let audit_events: Vec<serde_json::Value> = res.json().await.unwrap();
    assert_eq!(audit_events.len(), 1);
    assert_eq!(
        audit_events[0].get("event_type").unwrap(),
        "APPROVAL_DECIDED"
    );

    // 5. Verify Self-Learning Loop: Founder's decision was stored into permanent Memory as a verified precedent
    let learned_rec = store
        .get_record(
            &cos_memory::MemoryNamespace::Company,
            &format!("policy:precedent:{}", task_id),
        )
        .expect("Query memory failed")
        .expect("Learned memory precedent record should exist");
    assert_eq!(learned_rec.content["decision"], "approve");
    assert_eq!(
        learned_rec.lifecycle_status,
        cos_domain::KnowledgeLifecycle::Verified
    );
    assert_eq!(learned_rec.confidence, 1.0);
}

#[tokio::test]
async fn test_realtime_sse_event_stream() {
    let (base_url, _store) = spawn_test_server().await;
    let client = reqwest::Client::new();

    // Verify SSE endpoint responds with text/event-stream content type
    let res = client
        .get(format!("{}/api/v1/events/stream", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("text/event-stream"));
}

#[tokio::test]
async fn test_api_with_sqlite_backend() {
    let sqlite_store = Arc::new(cos_persistence::SqliteStore::in_memory().unwrap());
    let state = AppState::from_sqlite_store(sqlite_store);
    let router = build_app_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{}/api/v1/health", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let approvals_res = client
        .get(format!("http://{}/api/v1/approvals", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(approvals_res.status(), 200);
}
