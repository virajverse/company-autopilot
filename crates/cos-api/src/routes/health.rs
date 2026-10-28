use axum::{http::StatusCode, response::IntoResponse, Json};
use chrono::Utc;

pub async fn health_check() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "healthy",
            "service": "autonomous-company-os",
            "timestamp": Utc::now().to_rfc3339()
        })),
    )
}
