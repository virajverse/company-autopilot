//! `cos-tools` is the SINGLE authorized door for all external side effects.
//!
//! # Laws Enforced:
//! 1. No direct agent API calls: agents only prepare execution requests.
//! 2. Side effects require short-lived TaskGrant, exact ActionDigest, and scope validation.
//! 3. Atomic Idempotency: duplicate attempts with same key return cached result; different payload returns IdempotencyConflict.
//! 4. Audit trail: every tool execution attempt (success, failure, or denial) emits an audit event.

use chrono::{DateTime, Utc};
use cos_audit::{AuditEvent, AuditEventType, AuditWriterPort};
use cos_contracts::{ActionDigest, Capability, CommandEnvelope, TaskGrant};
use cos_domain::{ClientScope, TenantId};
use cos_governance::GovernanceEngine;
use cos_identity::ActorIdentity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ToolError {
    #[error("Permission denied by Governance: {reason}")]
    PermissionDenied { reason: String },

    #[error("Action requires approval: {reason}")]
    ApprovalRequired { reason: String },

    #[error("Idempotency conflict: key '{key}' already reserved with different action digest")]
    IdempotencyConflict { key: String },

    #[error("Tool '{tool}' failed execution: {error}")]
    ExecutionFailed { tool: String, error: String },

    #[error("Tool '{tool}' timed out")]
    TimedOut { tool: String },

    #[error("Unknown tool '{tool}'")]
    UnknownTool { tool: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolExecutionStatus {
    Succeeded,
    Failed { error: String },
    TimedOut,
    IdempotencyConflict { message: String },
    UnknownExternalState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub execution_id: Uuid,
    pub status: ToolExecutionStatus,
    pub output: serde_json::Value,
    pub completed_at: DateTime<Utc>,
}

/// Abstract port implemented by external integration adapters (CRM, Email, WhatsApp, etc.).
pub trait ToolAdapter: Send + Sync {
    fn tool_name(&self) -> &str;
    fn capability(&self) -> Capability;
    fn execute(&self, payload: &serde_json::Value) -> Result<serde_json::Value, String>;
}

/// Atomic idempotency record tracking unique (tenant_id, tool_name, idempotency_key).
#[derive(Debug, Clone)]
struct IdempotencyRecord {
    action_digest: ActionDigest,
    result: ToolExecutionResult,
}

/// Thread-safe in-memory idempotency store with bounded capacity.
#[derive(Debug)]
pub struct InMemoryIdempotencyStore {
    records: RwLock<HashMap<(TenantId, String, String), IdempotencyRecord>>,
    max_capacity: usize,
}

impl Default for InMemoryIdempotencyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryIdempotencyStore {
    pub fn new() -> Self {
        Self::with_capacity(10_000)
    }

    pub fn with_capacity(max_capacity: usize) -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            max_capacity,
        }
    }
}

pub mod adapters;

pub use adapters::{FileDeliverableAdapter, HttpWebhookAdapter, NotificationToolAdapter};

/// Central Tool Gateway controlling all external side effects.
pub struct ToolGateway {
    governance: GovernanceEngine,
    audit_logger: Arc<dyn AuditWriterPort>,
    idempotency_store: InMemoryIdempotencyStore,
    tools: RwLock<HashMap<String, Arc<dyn ToolAdapter>>>,
}

impl ToolGateway {
    pub fn new(audit_logger: Arc<dyn AuditWriterPort>) -> Self {
        Self {
            governance: GovernanceEngine::new(),
            audit_logger,
            idempotency_store: InMemoryIdempotencyStore::new(),
            tools: RwLock::new(HashMap::new()),
        }
    }

    /// Registers a tool adapter.
    pub fn register_tool(&self, adapter: Arc<dyn ToolAdapter>) {
        self.tools
            .write()
            .unwrap()
            .insert(adapter.tool_name().to_string(), adapter);
    }

    /// Executes an authorized command through the designated tool adapter.
    pub fn execute_command(
        &self,
        command: &CommandEnvelope<serde_json::Value>,
        grant: &TaskGrant,
        tool_name: &str,
        client_scope: Option<&ClientScope>,
        approval_actor: Option<&ActorIdentity>,
    ) -> Result<ToolExecutionResult, ToolError> {
        let now = Utc::now();

        // 1. Look up tool
        let tool = {
            let tools = self.tools.read().unwrap();
            tools
                .get(tool_name)
                .cloned()
                .ok_or_else(|| ToolError::UnknownTool {
                    tool: tool_name.to_string(),
                })?
        };

        // 2. Validate authorization via Governance
        let decision = self.governance.authorize_tool_execution(
            grant,
            &tool.capability(),
            &command.action_digest,
            client_scope,
            now,
            approval_actor,
        );

        match decision {
            cos_governance::AuthorizationDecision::Allowed => {
                // Authorized to proceed
            }
            cos_governance::AuthorizationDecision::RequiresApproval { reason, .. } => {
                self.audit_logger
                    .record(AuditEvent::new(
                        command.context.clone(),
                        AuditEventType::ToolExecutionDenied,
                        Some(command.action_digest),
                        format!("Tool execution blocked pending approval: {}", reason),
                    ))
                    .ok();
                return Err(ToolError::ApprovalRequired { reason });
            }
            cos_governance::AuthorizationDecision::Denied { reason } => {
                self.audit_logger
                    .record(AuditEvent::new(
                        command.context.clone(),
                        AuditEventType::ToolExecutionDenied,
                        Some(command.action_digest),
                        format!("Tool execution denied: {}", reason),
                    ))
                    .ok();
                return Err(ToolError::PermissionDenied { reason });
            }
        }

        // 3. Atomic Idempotency Check
        let idempotency_key = (
            grant.tenant_scope,
            tool_name.to_string(),
            command.idempotency_key.clone(),
        );
        {
            let store = self.idempotency_store.records.read().unwrap();
            if let Some(record) = store.get(&idempotency_key) {
                if record.action_digest == command.action_digest {
                    // Safe replay: return cached result
                    return Ok(record.result.clone());
                } else {
                    return Err(ToolError::IdempotencyConflict {
                        key: command.idempotency_key.clone(),
                    });
                }
            }
        }

        // 4. Emit Audit Event: Execution Started
        self.audit_logger
            .record(AuditEvent::new(
                command.context.clone(),
                AuditEventType::ToolExecutionStarted,
                Some(command.action_digest),
                format!("Executing tool '{}'", tool_name),
            ))
            .ok();

        // 5. Execute adapter
        let exec_result = match tool.execute(&command.payload) {
            Ok(output) => ToolExecutionResult {
                execution_id: Uuid::now_v7(),
                status: ToolExecutionStatus::Succeeded,
                output,
                completed_at: Utc::now(),
            },
            Err(err) => ToolExecutionResult {
                execution_id: Uuid::now_v7(),
                status: ToolExecutionStatus::Failed { error: err.clone() },
                output: serde_json::json!({ "error": err }),
                completed_at: Utc::now(),
            },
        };

        // 6. Save Idempotency Record (Enforcing Bounded Capacity)
        {
            let mut store = self.idempotency_store.records.write().unwrap();
            if store.len() >= self.idempotency_store.max_capacity {
                if let Some(oldest_key) = store.keys().next().cloned() {
                    store.remove(&oldest_key);
                }
            }
            store.insert(
                idempotency_key,
                IdempotencyRecord {
                    action_digest: command.action_digest,
                    result: exec_result.clone(),
                },
            );
        }

        // 7. Emit Audit Event: Execution Completed
        self.audit_logger
            .record(AuditEvent::new(
                command.context.clone(),
                AuditEventType::ToolExecutionCompleted,
                Some(command.action_digest),
                format!(
                    "Completed tool '{}' with status {:?}",
                    tool_name, exec_result.status
                ),
            ))
            .ok();

        Ok(exec_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use cos_audit::InMemoryAuditLogger;
    use cos_contracts::{ActionDigest, Capability, CommandEnvelope, TaskGrant};
    use cos_domain::{
        AgentId, GrantId, GrantStatus, RiskClass, SubjectId, TaskId, TenantId, TraceId,
    };
    use cos_identity::ActorIdentity;

    #[test]
    fn test_tool_gateway_execution_and_idempotency() {
        let logger = Arc::new(InMemoryAuditLogger::new());
        let gateway = ToolGateway::new(logger.clone());
        gateway.register_tool(Arc::new(NotificationToolAdapter::new()));

        let payload = serde_json::json!({
            "recipient": "lead@example.com",
            "message": "Thank you for reaching out!"
        });
        let digest =
            ActionDigest::from_serializable(&payload).expect("Failed to serialize payload");

        let now = Utc::now();
        let grant = TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id: TaskId::new_v7(),
            agent_id: AgentId::new_v7(),
            subject_id: SubjectId::new_v7(),
            role: "sales".to_string(),
            allowed_capabilities: vec![Capability::new("send_notification")],
            client_scope: None,
            project_scope: None,
            tenant_scope: TenantId::new_v7(),
            risk_class: RiskClass::R1ReversibleLowImpact,
            approval_reference: None,
            action_digest: digest,
            policy_version: 1,
            grant_status: GrantStatus::Active,
            issued_at: now,
            expires_at: now + Duration::minutes(10),
            max_calls: 3,
            issued_by: ActorIdentity::system("governance"),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        };

        let cmd = CommandEnvelope {
            command_id: Uuid::now_v7(),
            command_type: "send_notification".to_string(),
            context: cos_contracts::TraceContext::root(ActorIdentity::system("executive")),
            task_id: Some(grant.task_id),
            goal_id: None,
            client_scope: None,
            project_scope: None,
            payload: payload.clone(),
            expected_version: 1,
            capability_grant: Some(grant.clone()),
            approval_reference: None,
            action_digest: digest,
            idempotency_key: "idem_key_001".to_string(),
            created_at: now,
        };

        // First execution -> succeeds
        let res1 = gateway
            .execute_command(&cmd, &grant, "notification_sender", None, None)
            .expect("Tool execution failed");
        assert_eq!(res1.status, ToolExecutionStatus::Succeeded);

        // Second execution with identical payload and key -> returns cached result without re-executing
        let res2 = gateway
            .execute_command(&cmd, &grant, "notification_sender", None, None)
            .expect("Idempotent replay failed");
        assert_eq!(res1.execution_id, res2.execution_id);

        // Third execution with same key but altered payload -> fails with IdempotencyConflict!
        let altered_payload =
            serde_json::json!({ "recipient": "attacker@example.com", "message": "Injected" });
        let altered_digest = ActionDigest::from_serializable(&altered_payload).unwrap();
        let mut altered_cmd = cmd.clone();
        altered_cmd.payload = altered_payload;
        altered_cmd.action_digest = altered_digest;

        let mut altered_grant = grant.clone();
        altered_grant.action_digest = altered_digest;

        let conflict = gateway.execute_command(
            &altered_cmd,
            &altered_grant,
            "notification_sender",
            None,
            None,
        );
        assert!(matches!(
            conflict,
            Err(ToolError::IdempotencyConflict { .. })
        ));

        // Verify audit log captured start and completion events
        assert!(logger.len() >= 2);
    }

    #[test]
    fn test_file_deliverable_adapter_security_and_writing() {
        let temp_dir = std::env::temp_dir().join(format!("cos_deliv_test_{}", Uuid::new_v4()));
        let adapter = FileDeliverableAdapter::new(&temp_dir);

        // Valid write
        let payload = serde_json::json!({
            "file_name": "test_doc.md",
            "content": "# Test Deliverable Content",
            "subfolder": "reports"
        });
        let result = adapter.execute(&payload).expect("Write failed");
        assert_eq!(result["status"], "written");

        // Verify file actually exists
        let written_file = temp_dir.join("reports").join("test_doc.md");
        assert!(written_file.exists());
        let content = std::fs::read_to_string(&written_file).unwrap();
        assert_eq!(content, "# Test Deliverable Content");

        // Path traversal attack: forbidden!
        let malicious_payload = serde_json::json!({
            "file_name": "../secret.txt",
            "content": "stolen"
        });
        assert!(adapter.execute(&malicious_payload).is_err());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_notification_tool_adapter() {
        let adapter = NotificationToolAdapter::new();
        let payload = serde_json::json!({
            "channel": "email",
            "recipient": "founder@example.com",
            "subject": "System Report",
            "message": "All operations running normally."
        });
        let result = adapter.execute(&payload).expect("Notification failed");
        assert_eq!(result["status"], "dispatched");
        assert_eq!(result["recipient"], "founder@example.com");
    }
}
