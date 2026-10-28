use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use cos_persistence::AuditRepository;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct AuditPagination {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

pub async fn list_audit_events(
    State(state): State<AppState>,
    Query(pagination): Query<AuditPagination>,
) -> impl IntoResponse {
    match state.store.list_events(pagination.offset, pagination.limit) {
        Ok(events) => (StatusCode::OK, Json(events)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
