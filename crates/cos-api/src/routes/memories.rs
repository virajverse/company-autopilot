use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use cos_domain::ClientScope;
use cos_memory::{MemoryNamespace, MemoryRecord};
use cos_persistence::MemoryRepository;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MemoryQueryParams {
    pub namespace: Option<String>,
    pub key_prefix: Option<String>,
    pub client_scope: Option<String>,
    pub search: Option<String>,
}

/// Lists and queries memory records with namespace filtering and search.
pub async fn query_memories(
    State(state): State<AppState>,
    Query(params): Query<MemoryQueryParams>,
) -> impl IntoResponse {
    let namespace = match params.namespace.as_deref() {
        Some("client") => {
            if let Some(scope_str) = params.client_scope {
                if let Ok(scope_uuid) = scope_str.parse() {
                    MemoryNamespace::Client(ClientScope::from_uuid(scope_uuid))
                } else {
                    MemoryNamespace::Company
                }
            } else {
                MemoryNamespace::Company
            }
        }
        _ => MemoryNamespace::Company,
    };

    match state.store.list_records(&namespace) {
        Ok(records) => {
            let mut filtered: Vec<_> = records;

            if let Some(ref prefix) = params.key_prefix {
                filtered.retain(|r| r.key.starts_with(prefix));
            }

            if let Some(ref search_query) = params.search {
                let query_lower = search_query.to_lowercase();
                filtered.retain(|r| {
                    r.key.to_lowercase().contains(&query_lower)
                        || r.content.to_string().to_lowercase().contains(&query_lower)
                });
            }

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "count": filtered.len(),
                    "records": filtered
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

/// Dynamically injects or updates a memory record via API.
pub async fn save_memory_record(
    State(state): State<AppState>,
    Json(record): Json<MemoryRecord>,
) -> impl IntoResponse {
    match state.store.save_record(&record) {
        Ok(_) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "message": "Memory record saved successfully",
                "key": record.key,
                "memory_id": record.memory_id
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
