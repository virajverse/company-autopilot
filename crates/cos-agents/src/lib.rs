//! `cos-agents` provides generic, dynamic role-based agent workers.
//!
//! # Architecture Invariant:
//! Zero hardcoded business logic, thresholds, or templates in agent code.
//! All business guidelines, instructions, and procedures are supplied dynamically via MemoryRecords.

use cos_contracts::{ActionDigest, Capability, TaskGrant};
use cos_domain::{AgentId, RiskClass, TaskId};
use cos_memory::MemoryRecord;
use serde::{Deserialize, Serialize};

/// High-level profile metadata for an agent role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub role_name: String,
    pub mission: String,
    pub capabilities: Vec<Capability>,
}

/// Structured proposal emitted by a dynamic agent worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExecutionProposal {
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub target_tool: String,
    pub payload: serde_json::Value,
    pub action_digest: ActionDigest,
    pub risk_class: RiskClass,
}

/// Generic dynamic agent worker whose behavior is governed entirely by memory guidelines and task grants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicAgent {
    pub agent_id: AgentId,
    pub role_name: String,
    pub mission: String,
    pub capabilities: Vec<Capability>,
}

impl DynamicAgent {
    pub fn new(
        role_name: impl Into<String>,
        mission: impl Into<String>,
        capabilities: Vec<Capability>,
    ) -> Self {
        Self {
            agent_id: AgentId::new_v7(),
            role_name: role_name.into(),
            mission: mission.into(),
            capabilities,
        }
    }

    pub fn profile(&self) -> AgentProfile {
        AgentProfile {
            role_name: self.role_name.clone(),
            mission: self.mission.clone(),
            capabilities: self.capabilities.clone(),
        }
    }

    /// Evaluates a task and produces an execution proposal based on retrieved Memory guidelines.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_task_with_memory(
        &self,
        task_id: TaskId,
        task_payload: &serde_json::Value,
        grant: &TaskGrant,
        guidelines_record: Option<&MemoryRecord>,
        target_tool: impl Into<String>,
        required_capability: &Capability,
        estimated_risk: RiskClass,
    ) -> Result<AgentExecutionProposal, String> {
        // Enforce grant capability
        if !grant.has_capability(required_capability) {
            return Err(format!(
                "Agent grant does not include required capability '{}'",
                required_capability
            ));
        }

        let target_tool = target_tool.into();

        // Extract guidance from dynamic memory record if present
        let guidance_payload = guidelines_record
            .map(|r| r.content.clone())
            .unwrap_or(serde_json::Value::Null);

        // Construct dynamic proposal combining task input and dynamic memory guidance
        let proposal_payload = serde_json::json!({
            "task_data": task_payload,
            "guidance_applied": guidance_payload,
            "processed_by_role": self.role_name,
        });

        let digest = ActionDigest::from_serializable(&proposal_payload)
            .map_err(|e| format!("Action digest calculation failed: {}", e))?;

        Ok(AgentExecutionProposal {
            task_id,
            agent_id: self.agent_id,
            target_tool,
            payload: proposal_payload,
            action_digest: digest,
            risk_class: estimated_risk,
        })
    }

    /// Performs cognitive reasoning using an LLM to generate structured tool execution proposals,
    /// guided by Memory SOP guidelines and strictly bounded by the TaskGrant.
    #[allow(clippy::too_many_arguments)]
    pub async fn reason_with_llm(
        &self,
        llm: &dyn cos_llm::LlmPort,
        model: Option<String>,
        task_id: TaskId,
        task_payload: &serde_json::Value,
        grant: &TaskGrant,
        guidelines_record: Option<&MemoryRecord>,
        target_tool: impl Into<String>,
        required_capability: &Capability,
        estimated_risk: RiskClass,
    ) -> Result<AgentExecutionProposal, String> {
        // 1. Enforce grant capability invariant
        if !grant.has_capability(required_capability) {
            return Err(format!(
                "Agent grant does not include required capability '{}'",
                required_capability
            ));
        }

        let target_tool = target_tool.into();

        // 2. Assemble system prompt from agent role mission and dynamic memory SOP
        let guidance_text = guidelines_record
            .map(|r| serde_json::to_string_pretty(&r.content).unwrap_or_default())
            .unwrap_or_else(|| {
                "No specific guidelines provided. Follow general business best practices."
                    .to_string()
            });

        let system_prompt = format!(
            "You are an autonomous AI agent working in the role of '{}'.\n\
             Mission: {}\n\n\
             OPERATIONAL SOP & GUIDELINES (from Memory):\n{}\n\n\
             TASK OBJECTIVE: Formulate structured JSON execution parameters for tool '{}'.\n\
             INVARIANT: You MUST return ONLY valid JSON matching parameters for '{}'. Do NOT include conversational filler.",
            self.role_name, self.mission, guidance_text, target_tool, target_tool
        );

        let user_prompt = format!(
            "Task ID: {}\nInput Data:\n{}",
            task_id,
            serde_json::to_string_pretty(task_payload).unwrap_or_default()
        );

        let req = cos_llm::CompletionRequest {
            model,
            messages: vec![
                cos_llm::ChatMessage::system(system_prompt),
                cos_llm::ChatMessage::user(user_prompt),
            ],
            temperature: Some(0.2),
            max_tokens: Some(2048),
            response_format_json: true,
        };

        // 3. Invoke LLM Port
        let response = llm
            .complete(req)
            .await
            .map_err(|e| format!("LLM reasoning invocation failed: {}", e))?;

        // 4. Parse JSON payload
        let cleaned_content = clean_json_text(&response.content);
        let proposal_payload: serde_json::Value =
            serde_json::from_str(&cleaned_content).map_err(|e| {
                format!(
                    "LLM response was not valid JSON: '{}' (error: {})",
                    response.content, e
                )
            })?;

        // 5. Deterministically compute ActionDigest
        let digest = ActionDigest::from_serializable(&proposal_payload)
            .map_err(|e| format!("Failed to compute action digest: {}", e))?;

        Ok(AgentExecutionProposal {
            task_id,
            agent_id: self.agent_id,
            target_tool,
            payload: proposal_payload,
            action_digest: digest,
            risk_class: estimated_risk,
        })
    }
}

fn clean_json_text(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(stripped) = trimmed.strip_prefix("```json") {
        if let Some(inner) = stripped.strip_suffix("```") {
            return inner.trim().to_string();
        }
    }
    if let Some(stripped) = trimmed.strip_prefix("```") {
        if let Some(inner) = stripped.strip_suffix("```") {
            return inner.trim().to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use cos_domain::{GrantId, GrantStatus, SubjectId, TenantId, TraceId};
    use cos_identity::ActorIdentity;
    use cos_memory::{MemoryKind, MemoryNamespace, ProvenanceType};

    #[test]
    fn test_dynamic_agent_proposal_with_memory() {
        let agent = DynamicAgent::new(
            "generic_operations",
            "Process tasks based on dynamic memory",
            vec![Capability::new("external_dispatch")],
        );

        let task_id = TaskId::new_v7();
        let now = Utc::now();
        let digest = ActionDigest::compute(b"test_digest");

        let grant = TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id,
            agent_id: agent.agent_id,
            subject_id: cos_domain::SubjectId::new_v7(),
            role: agent.role_name.clone(),
            allowed_capabilities: vec![Capability::new("external_dispatch")],
            client_scope: None,
            project_scope: None,
            tenant_scope: TenantId::new_v7(),
            risk_class: RiskClass::R1ReversibleLowImpact,
            approval_reference: None,
            action_digest: digest,
            policy_version: 1,
            grant_status: GrantStatus::Active,
            issued_at: now,
            expires_at: now + Duration::minutes(15),
            max_calls: 3,
            issued_by: ActorIdentity::system("governance"),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        };

        // Create a completely custom guideline in memory
        let guideline = MemoryRecord::new(
            MemoryNamespace::Company,
            MemoryKind::Guideline,
            "custom_sop",
            serde_json::json!({ "rule": "always verify contact before dispatch" }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            ProvenanceType::Learned,
            ActorIdentity::system("admin"),
        );

        let task_input =
            serde_json::json!({ "customer_inquiry": "Need software architecture assistance" });

        let proposal = agent
            .evaluate_task_with_memory(
                task_id,
                &task_input,
                &grant,
                Some(&guideline),
                "dispatcher_tool",
                &Capability::new("external_dispatch"),
                RiskClass::R1ReversibleLowImpact,
            )
            .expect("Failed to evaluate task");

        assert_eq!(proposal.target_tool, "dispatcher_tool");
        assert_eq!(proposal.risk_class, RiskClass::R1ReversibleLowImpact);
        assert_eq!(
            proposal.payload["guidance_applied"]["rule"],
            "always verify contact before dispatch"
        );
    }

    struct TestFixedLlm;

    #[async_trait::async_trait]
    impl cos_llm::LlmPort for TestFixedLlm {
        async fn verify_connection(
            &self,
        ) -> Result<cos_llm::ConnectionVerifyResult, cos_llm::LlmError> {
            Ok(cos_llm::ConnectionVerifyResult {
                success: true,
                latency_ms: 10,
                message: "Connection verified".to_string(),
                detected_server: Some("test-inference".to_string()),
            })
        }

        async fn discover_models(
            &self,
        ) -> Result<Vec<cos_llm::DiscoveredModel>, cos_llm::LlmError> {
            Ok(Vec::new())
        }

        async fn complete(
            &self,
            _req: cos_llm::CompletionRequest,
        ) -> Result<cos_llm::CompletionResponse, cos_llm::LlmError> {
            Ok(cos_llm::CompletionResponse {
                content: serde_json::to_string(&serde_json::json!({
                    "action": "send_proposal",
                    "tier": "enterprise",
                    "estimated_hours": 120
                }))
                .unwrap(),
                model: "test-model".to_string(),
                usage: Some(cos_llm::TokenUsage::default()),
                finish_reason: Some("stop".to_string()),
            })
        }
    }

    #[tokio::test]
    async fn test_agent_reason_with_llm() {
        let test_llm = TestFixedLlm;

        let agent = DynamicAgent::new(
            "proposal_agent",
            "Generate enterprise software proposals",
            vec![Capability::new("external_dispatch")],
        );

        let task_id = TaskId::new_v7();
        let now = Utc::now();
        let digest = ActionDigest::compute(b"test_digest_llm");

        let grant = TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id,
            agent_id: agent.agent_id,
            subject_id: SubjectId::new_v7(),
            role: "proposal_agent".to_string(),
            allowed_capabilities: vec![Capability::new("external_dispatch")],
            client_scope: None,
            project_scope: None,
            tenant_scope: TenantId::new_v7(),
            risk_class: RiskClass::R2ExternalCommunication,
            approval_reference: None,
            action_digest: digest,
            policy_version: 1,
            grant_status: GrantStatus::Active,
            issued_at: now,
            expires_at: now + Duration::minutes(15),
            max_calls: 3,
            issued_by: ActorIdentity::system("governance"),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        };

        let task_input = serde_json::json!({ "client_need": "Cloud migration plan" });

        let proposal = agent
            .reason_with_llm(
                &test_llm,
                None,
                task_id,
                &task_input,
                &grant,
                None,
                "proposal_tool",
                &Capability::new("external_dispatch"),
                RiskClass::R2ExternalCommunication,
            )
            .await
            .expect("Reasoning failed");

        assert_eq!(proposal.target_tool, "proposal_tool");
        assert_eq!(proposal.payload["action"], "send_proposal");
        assert_eq!(proposal.payload["estimated_hours"], 120);
    }
}
