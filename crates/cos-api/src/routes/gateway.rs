use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use cos_llm::{ConnectionVerifyResult, LlmGatewayConfig};
use cos_persistence::GatewayConfigRepository;

/// Returns current LLM Gateway configuration with masked API keys for UI display.
pub async fn get_gateway_config(State(state): State<AppState>) -> impl IntoResponse {
    match state.store.load_config() {
        Ok(cfg) => (StatusCode::OK, Json(cfg.to_ui_safe())).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Updates LLM Gateway configuration from UI settings form.
pub async fn update_gateway_config(
    State(state): State<AppState>,
    Json(payload): Json<LlmGatewayConfig>,
) -> impl IntoResponse {
    match state.update_llm_gateway(payload) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "message": "Gateway configuration updated successfully" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// Triggers connection verification to the configured gateway base URL.
pub async fn verify_gateway_connection(State(state): State<AppState>) -> impl IntoResponse {
    let port = state.llm_port.read().unwrap().clone();
    match port.verify_connection().await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (
            StatusCode::OK,
            Json(ConnectionVerifyResult {
                success: false,
                latency_ms: 0,
                message: e.to_string(),
                detected_server: None,
            }),
        )
            .into_response(),
    }
}

/// Backwards-compatible alias for UI clients calling test-connection.
pub async fn test_gateway_connection(state: State<AppState>) -> impl IntoResponse {
    verify_gateway_connection(state).await
}

/// Discovers available models from the inference gateway endpoint.
pub async fn discover_models(State(state): State<AppState>) -> impl IntoResponse {
    let port = state.llm_port.read().unwrap().clone();
    match port.discover_models().await {
        Ok(models) => (StatusCode::OK, Json(models)).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
