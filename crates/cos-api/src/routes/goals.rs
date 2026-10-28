use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use cos_goals::Goal;
use cos_persistence::GoalRepository;

pub async fn list_goals(State(state): State<AppState>) -> impl IntoResponse {
    match state.store.list_all() {
        Ok(goals) => (StatusCode::OK, Json(goals)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn create_goal(
    State(state): State<AppState>,
    Json(goal): Json<Goal>,
) -> impl IntoResponse {
    match state.store.save_goal(&goal) {
        Ok(_) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "id": goal.id.0, "goal": goal })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
