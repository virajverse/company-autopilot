use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use cos_memory::MemoryNamespace;
use cos_persistence::MemoryRepository;

/// Returns all active specialist agents dynamically registered in the system.
pub async fn list_agents(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(ref exec) = state.executive {
        let agents = exec.list_registered_agents();
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "count": agents.len(),
                "agents": agents
            })),
        )
            .into_response()
    } else {
        // Fallback: discover from memory store directly
        match state.store.list_records(&MemoryNamespace::Company) {
            Ok(records) => {
                let sops: Vec<_> = records
                    .into_iter()
                    .filter(|r| r.key.starts_with("sop:"))
                    .collect();
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "count": sops.len(),
                        "sops": sops
                    })),
                )
                    .into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    }
}
