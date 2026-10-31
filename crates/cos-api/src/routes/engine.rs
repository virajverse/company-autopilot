use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};

/// Returns real-time autonomous engine execution status and metrics.
pub async fn get_engine_status(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(ref engine) = state.engine {
        (StatusCode::OK, Json(engine.get_status_summary())).into_response()
    } else {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "idle",
                "message": "Engine running without live API inspection pointer"
            })),
        )
            .into_response()
    }
}

/// Triggers an immediate discrete execution tick of the autonomous engine.
pub async fn trigger_engine_tick(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(ref engine) = state.engine {
        match engine.tick() {
            Ok(summary) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "tick_summary": summary
                })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "status": "error",
                    "error": e.to_string()
                })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({
                "status": "unavailable",
                "message": "Autonomous engine is not attached to this API instance"
            })),
        )
            .into_response()
    }
}
