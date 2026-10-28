mod bootstrap;

use cos_agents::DynamicAgent;
use cos_api::{build_app_router, AppState};
use cos_contracts::Capability;
use cos_domain::TenantId;
use cos_executive::{AutonomousEngine, CompanyExecutive};
use cos_goals::GoalForest;
use cos_learning::LearningEngine;
use cos_persistence::PersistenceEngine;
use cos_tools::{FileDeliverableAdapter, HttpWebhookAdapter, NotificationToolAdapter, ToolGateway};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize structured logging
    tracing_subscriber::fmt::init();

    info!("--------------------------------------------------");
    info!("Viraj Verse - Autonomous Company Operating System");
    info!("Architecture: Memory-Driven, Zero-Hardcoded Copy");
    info!("Security: R0-R4 Governance & Strict Human Approvals");
    info!("Engine: Autonomous Heartbeat Daemon + Real-time SSE");
    info!("--------------------------------------------------");

    // 2. Initialize durable persistent storage (Default: SQLite WAL mode on CPU for enterprise scale)
    let data_dir = std::env::var("COS_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let backend_type =
        std::env::var("COS_STORAGE_BACKEND").unwrap_or_else(|_| "sqlite".to_string());
    info!(
        "Initializing storage engine (backend: {}) at: {}",
        backend_type, data_dir
    );

    let store = match backend_type.to_lowercase().as_str() {
        "file" => Arc::new(PersistenceEngine::file(&data_dir)?),
        _ => {
            let db_path = std::path::Path::new(&data_dir).join("cos.db");
            Arc::new(PersistenceEngine::sqlite(db_path)?)
        }
    };
    let mem_store = Arc::new(cos_memory::MemoryStore::new());

    // 3. Seed dynamic company memory on cold-start (4 canonical MVP roles & HTN templates)
    bootstrap::seed_company_memory(&*store, &mem_store)?;

    // 4. Initialize Tool Gateway and register standard production adapters
    let tool_gateway = Arc::new(ToolGateway::new(store.clone()));
    tool_gateway.register_tool(Arc::new(FileDeliverableAdapter::default_dir()));
    tool_gateway.register_tool(Arc::new(NotificationToolAdapter::new()));
    tool_gateway.register_tool(Arc::new(HttpWebhookAdapter::new()));

    // 5. Initialize central Company Executive runtime
    let tenant_id = TenantId::new_v7();
    let executive = Arc::new(CompanyExecutive::new(
        tenant_id,
        store.clone(),
        mem_store,
        tool_gateway,
    ));

    // 6. Register 4 canonical MVP specialist agent roles
    executive.register_agent(DynamicAgent::new(
        "sales_and_lead",
        "Qualify inbound inquiries and prepare structured follow-up proposals",
        vec![
            Capability::new("send_notification"),
            Capability::new("network_http"),
        ],
    ));
    executive.register_agent(DynamicAgent::new(
        "discovery_and_proposal",
        "Translate client requirements into technical specifications and work proposals",
        vec![
            Capability::new("filesystem_write"),
            Capability::new("send_notification"),
        ],
    ));
    executive.register_agent(DynamicAgent::new(
        "project_operations",
        "Coordinate task DAG execution, milestone validation, and deliverable handoffs",
        vec![
            Capability::new("filesystem_write"),
            Capability::new("network_http"),
        ],
    ));
    executive.register_agent(DynamicAgent::new(
        "client_success_communication",
        "Maintain client relationship health, onboarding satisfaction, and delivery check-ins",
        vec![Capability::new("send_notification")],
    ));

    // 7. Initialize Goal Forest & Learning Engine
    let forest = Arc::new(RwLock::new(GoalForest::new()));
    let learning = Arc::new(LearningEngine::new());

    // 8. Launch Autonomous Heartbeat Daemon in background
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    let engine = Arc::new(AutonomousEngine::new(executive, forest, learning));
    let engine_worker = engine.clone();
    tokio::spawn(async move {
        engine_worker
            .run_daemon(std::time::Duration::from_secs(5), shutdown_rx)
            .await;
    });

    // 9. Construct application state and API router
    let app_state = AppState::new(store);
    let router = build_app_router(app_state);

    // 10. Resolve listening host and port
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let ip: std::net::IpAddr = host
        .parse()
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)));

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8008);

    let addr = SocketAddr::from((ip, port));
    info!("COS REST & Gateway API listening on http://{}", addr);
    info!("- Gateway Config:      GET/PUT /api/v1/gateway/config");
    info!("- Verify Connection:   POST    /api/v1/gateway/verify-connection");
    info!("- Test Connection:     POST    /api/v1/gateway/test-connection");
    info!("- Model Discovery:     GET     /api/v1/gateway/discover-models");
    info!("- Real-time Stream:    GET     /api/v1/events/stream");
    info!("- Approvals Inbox:     GET     /api/v1/approvals");
    info!("- Goal Forest:         GET/POST/api/v1/goals");
    info!("- Audit Stream:        GET     /api/v1/audit");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // 11. Serve with graceful shutdown
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await?;

    info!("COS Node shut down cleanly.");
    Ok(())
}

async fn shutdown_signal(shutdown_tx: tokio::sync::broadcast::Sender<()>) {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C signal handler");
    info!("Shutdown signal received, initiating graceful termination...");
    let _ = shutdown_tx.send(());
}
