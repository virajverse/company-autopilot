use crate::routes::{approvals, audit, events, gateway, goals, health};
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};

pub fn build_app_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Health
        .route("/api/v1/health", get(health::health_check))
        // Real-Time Server-Sent Events (SSE) Stream
        .route("/api/v1/events/stream", get(events::stream_events))
        // LLM Gateway Config & Tools (100% UI Match)
        .route(
            "/api/v1/gateway/config",
            get(gateway::get_gateway_config).put(gateway::update_gateway_config),
        )
        .route(
            "/api/v1/gateway/verify-connection",
            post(gateway::verify_gateway_connection),
        )
        .route(
            "/api/v1/gateway/test-connection",
            post(gateway::test_gateway_connection),
        )
        .route(
            "/api/v1/gateway/discover-models",
            get(gateway::discover_models),
        )
        // Approvals Inbox
        .route("/api/v1/approvals", get(approvals::list_pending_approvals))
        .route(
            "/api/v1/approvals/:id/decide",
            post(approvals::decide_approval),
        )
        // Goals Forest
        .route(
            "/api/v1/goals",
            get(goals::list_goals).post(goals::create_goal),
        )
        // Audit Trail
        .route("/api/v1/audit", get(audit::list_audit_events))
        .layer(cors)
        .with_state(state)
}
