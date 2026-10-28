use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use cos_audit::{AuditEvent, AuditEventType};
use cos_domain::{ApprovalId, SubjectId};
use cos_identity::ActorIdentity;
use cos_persistence::{ApprovalRepository, AuditRepository};
use serde::Deserialize;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct DecisionPayload {
    pub decision: String, // "approve" or "reject"
    #[serde(default)]
    pub reason: Option<String>,
    pub actor_id: String,
}

pub async fn list_pending_approvals(State(state): State<AppState>) -> impl IntoResponse {
    match state.store.list_pending() {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn decide_approval(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Json(payload): Json<DecisionPayload>,
) -> impl IntoResponse {
    let uuid = match Uuid::from_str(&id_str) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid approval UUID" })),
            )
                .into_response()
        }
    };
    let approval_id = ApprovalId(uuid);

    let mut request = match state.store.get_approval(&approval_id) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Approval request not found" })),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let subject_uuid = Uuid::from_str(&payload.actor_id).unwrap_or_else(|_| Uuid::now_v7());
    let approver = ActorIdentity::founder(SubjectId(subject_uuid), None);
    let now = chrono::Utc::now();
    let reason = payload
        .reason
        .unwrap_or_else(|| format!("Decision recorded via API by {}", payload.actor_id));

    let dec = payload.decision.trim().to_lowercase();
    match dec.as_str() {
        "approve" | "approved" | "accept" | "accepted" | "yes" => {
            if let Err(e) = request.approve(approver, reason.clone(), now) {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        }
        "reject" | "rejected" | "deny" | "denied" | "no" => {
            if let Err(e) = request.reject(approver, reason.clone(), now) {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid decision '{}'. Expected approve/reject, accept/deny, or yes/no", dec)
                })),
            )
                .into_response()
        }
    }

    if let Err(e) = state.store.save_approval(&request) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    // IMMUTABLE AUDIT TRAIL: Record the approval decision event
    let audit_event = AuditEvent::new(
        request.context.clone(),
        AuditEventType::ApprovalDecided,
        Some(request.action_digest),
        format!(
            "Approval '{}' decided as '{}' by actor '{}'. Reason: {}",
            request.approval_id.0, dec, payload.actor_id, reason
        ),
    );
    let _ = state.store.append_event(&audit_event);

    // REAL-TIME BROADCAST: Notify all connected frontend dashboards
    state.broadcast_event(
        "approval_decided",
        serde_json::json!({
            "approval_id": request.approval_id.0,
            "decision": dec,
            "status": request.status,
            "actor_id": payload.actor_id,
        }),
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "message": "Decision recorded successfully",
            "approval": request
        })),
    )
        .into_response()
}
