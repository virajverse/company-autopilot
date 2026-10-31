use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use cos_goals::GoalId;

/// Returns the execution plan DAG for a specific goal.
pub async fn get_plan_by_goal_id(
    State(state): State<AppState>,
    Path(goal_id_str): Path<String>,
) -> impl IntoResponse {
    let goal_id = match goal_id_str.parse::<uuid::Uuid>() {
        Ok(u) => GoalId(u),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Invalid goal UUID: {}", e) })),
            )
                .into_response();
        }
    };

    if let Some(ref engine) = state.engine {
        let plans = engine.get_plans();
        if let Some(plan) = plans.get(&goal_id) {
            (StatusCode::OK, Json(plan)).into_response()
        } else {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("No active execution plan found for goal '{}'", goal_id_str)
                })),
            )
                .into_response()
        }
    } else {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({
                "error": "Autonomous engine is not attached to this API instance"
            })),
        )
            .into_response()
    }
}
