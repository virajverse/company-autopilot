//! `cos-executive` is the central coordination runtime of the Company Operating System.
//!
//! # Architecture Invariant:
//! Zero hardcoded business logic or fixed agent personas.
//! All business guidelines, policies, and procedures are retrieved dynamically from Memory.

use chrono::{DateTime, Duration, Utc};
use cos_agents::DynamicAgent;
use cos_approval::ApprovalRequest;
use cos_audit::{AuditEvent, AuditEventType, AuditWriterPort};
use cos_config::GovernanceLimitsConfig;
use cos_contracts::{Capability, CommandEnvelope, TaskGrant, TraceContext};
use cos_domain::{
    AgentId, ClientScope, GrantId, GrantStatus, RiskClass, SubjectId, TaskId, TaskStatus, TenantId,
    TraceId,
};
use cos_identity::ActorIdentity;
use cos_memory::{MemoryNamespace, MemoryStore};
use cos_tools::{ToolExecutionResult, ToolGateway};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use uuid::Uuid;

pub mod engine;

pub use engine::{AutonomousEngine, TickSummary};

#[derive(Debug, Error)]
pub enum ExecutiveError {
    #[error("Task error: {0}")]
    TaskError(String),

    #[error("Unknown agent role: '{0}'")]
    UnknownRole(String),

    #[error("Tool execution error: {0}")]
    ToolError(#[from] cos_tools::ToolError),

    #[error("Approval required before execution")]
    PendingApproval(Box<ApprovalRequest>),

    #[error("Action rejected by approver: {0}")]
    ApprovalRejected(String),
}

/// Parameters for issuing a scoped grant.
#[derive(Debug, Clone)]
pub struct GrantRequestParams {
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub role: String,
    pub capabilities: Vec<Capability>,
    pub risk_class: RiskClass,
    pub digest: cos_contracts::ActionDigest,
    pub client_scope: Option<ClientScope>,
    pub validity_minutes: i64,
    pub max_calls: u32,
    pub policy_version: u32,
    pub now: DateTime<Utc>,
}

/// The outcome of evaluating the 4-tier organizational escalation ladder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscalationResolution {
    /// Tier 1: Specialist autonomous execution within local role grant & memory.
    Tier1Autonomous {
        role: String,
        reason: String,
    },
    /// Tier 2: Consult Executive / Reporting Manager memory policy.
    Tier2ManagerConsultation {
        policy_key: String,
        guidance: String,
    },
    /// Tier 3: Trigger external web/document intelligence research.
    Tier3WebResearchNeeded {
        query: String,
        target_role: String,
    },
    /// Tier 4: Founder cryptographic sign-off required.
    Tier4FounderApprovalRequired {
        action_digest: cos_contracts::ActionDigest,
        risk_class: RiskClass,
        summary: String,
    },
}

/// Parameters for processing a generic task.
#[derive(Debug, Clone)]
pub struct ProcessTaskParams {
    pub task_title: String,
    pub target_role: String,
    pub task_payload: serde_json::Value,
    pub memory_guideline_key: Option<String>,
    pub target_tool: String,
    pub required_capability: Capability,
    pub risk_class: RiskClass,
    pub client_scope: Option<ClientScope>,
    pub custom_grant_duration_minutes: Option<i64>,
    pub custom_approval_duration_hours: Option<i64>,
    pub custom_max_calls: Option<u32>,
}

/// Parameters for dispatching an action to the Tool Gateway.
#[derive(Debug, Clone)]
pub struct DispatchActionParams {
    pub approval: Option<ApprovalRequest>,
    pub approver: Option<ActorIdentity>,
    pub target_role: String,
    pub tool_name: String,
    pub capability: Capability,
    pub payload: serde_json::Value,
    pub digest: cos_contracts::ActionDigest,
    pub risk_class: RiskClass,
    pub client_scope: Option<ClientScope>,
    pub idempotency_key: String,
}

/// Task record managed by the Executive state machine.
#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub task_id: TaskId,
    pub title: String,
    pub assigned_agent: Option<AgentId>,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
}

/// Central Executive runtime orchestrating generic tasks with Memory.
pub struct CompanyExecutive {
    pub executive_identity: ActorIdentity,
    pub tenant_id: TenantId,
    pub audit_logger: Arc<dyn AuditWriterPort>,
    pub memory_store: Arc<MemoryStore>,
    pub tool_gateway: Arc<ToolGateway>,
    pub governance_config: GovernanceLimitsConfig,
    agents: RwLock<HashMap<String, DynamicAgent>>,
}

impl CompanyExecutive {
    pub fn new(
        tenant_id: TenantId,
        audit_logger: Arc<dyn AuditWriterPort>,
        memory_store: Arc<MemoryStore>,
        tool_gateway: Arc<ToolGateway>,
    ) -> Self {
        Self::with_config(
            tenant_id,
            audit_logger,
            memory_store,
            tool_gateway,
            GovernanceLimitsConfig::default(),
        )
    }

    pub fn with_config(
        tenant_id: TenantId,
        audit_logger: Arc<dyn AuditWriterPort>,
        memory_store: Arc<MemoryStore>,
        tool_gateway: Arc<ToolGateway>,
        governance_config: GovernanceLimitsConfig,
    ) -> Self {
        Self {
            executive_identity: ActorIdentity::executive(SubjectId::new_v7()),
            tenant_id,
            audit_logger,
            memory_store,
            tool_gateway,
            governance_config,
            agents: RwLock::new(HashMap::new()),
        }
    }

    /// Registers a dynamic agent role with the Executive.
    pub fn register_agent(&self, agent: DynamicAgent) {
        self.agents
            .write()
            .unwrap()
            .insert(agent.role_name.clone(), agent);
    }

    /// Returns an existing agent or dynamically loads and instantiates it from Memory.
    pub fn get_or_load_agent(&self, role_name: &str) -> Result<DynamicAgent, ExecutiveError> {
        {
            let agents = self.agents.read().unwrap();
            if let Some(agent) = agents.get(role_name) {
                return Ok(agent.clone());
            }
        }

        // Attempt to load from Memory (e.g. "sop:<role_name>" or "role:<role_name>")
        let sop_key = format!("sop:{}", role_name);
        let record_opt = self
            .memory_store
            .get_by_key(&MemoryNamespace::Company, &sop_key, None)
            .ok()
            .flatten();

        if let Some(record) = record_opt {
            let mission = record
                .content
                .get("mission")
                .and_then(|v| v.as_str())
                .unwrap_or("Autonomous Specialist Role")
                .to_string();

            // Extract capabilities from allowed_tools or explicit capabilities
            let mut capabilities = Vec::new();
            if let Some(caps) = record
                .content
                .get("capabilities")
                .and_then(|v| v.as_array())
            {
                for c in caps {
                    if let Some(s) = c.as_str() {
                        capabilities.push(Capability::new(s));
                    }
                }
            } else if let Some(tools) = record
                .content
                .get("allowed_tools")
                .and_then(|v| v.as_array())
            {
                for t in tools {
                    if let Some(tool_str) = t.as_str() {
                        match tool_str {
                            "notification_sender" => {
                                capabilities.push(Capability::new("send_notification"))
                            }
                            "file_deliverable" => {
                                capabilities.push(Capability::new("filesystem_write"))
                            }
                            "http_webhook" => capabilities.push(Capability::new("network_http")),
                            "web_research" => capabilities.push(Capability::new("web_research")),
                            _ => capabilities.push(Capability::new(tool_str)),
                        }
                    }
                }
            }

            if capabilities.is_empty() {
                capabilities.push(Capability::new("filesystem_write"));
            }

            let new_agent = DynamicAgent::new(role_name, mission, capabilities);
            self.register_agent(new_agent.clone());
            return Ok(new_agent);
        }

        Err(ExecutiveError::UnknownRole(role_name.to_string()))
    }

    /// Dynamically discovers and registers all specialist agents defined in Company Memory.
    pub fn load_all_agents_from_memory(&self) -> Result<usize, ExecutiveError> {
        let records = self
            .memory_store
            .query_records(&MemoryNamespace::Company, None, None)
            .map_err(|e| {
                ExecutiveError::TaskError(format!("Failed to query company memory: {}", e))
            })?;

        let mut loaded = 0;
        for record in records {
            if record.key.starts_with("sop:") {
                let role_name = record
                    .content
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| record.key.trim_start_matches("sop:"));

                if self.get_or_load_agent(role_name).is_ok() {
                    loaded += 1;
                }
            }
        }
        Ok(loaded)
    }

    /// Returns a list of all dynamically registered specialist agents.
    pub fn list_registered_agents(&self) -> Vec<DynamicAgent> {
        let agents = self.agents.read().unwrap();
        agents.values().cloned().collect()
    }

    /// Evaluates a task scenario through the strict 4-Tier Escalation Ladder:
    /// Tier 1 (Specialist) -> Tier 2 (Manager SOP) -> Tier 3 (Web Research) -> Tier 4 (Founder Approval)
    pub fn evaluate_escalation_ladder(
        &self,
        task_title: &str,
        target_role: &str,
        risk_class: RiskClass,
        payload: &serde_json::Value,
    ) -> Result<EscalationResolution, ExecutiveError> {
        // Tier 1: Low-risk tasks within autonomous specialist grant
        if matches!(
            risk_class,
            RiskClass::R0InternalRead | RiskClass::R1ReversibleLowImpact
        ) {
            return Ok(EscalationResolution::Tier1Autonomous {
                role: target_role.to_string(),
                reason: format!(
                    "Risk class {:?} allows autonomous specialist execution within grant",
                    risk_class
                ),
            });
        }

        // Tier 2: Check domain manager escalation policy in Memory
        let manager_policy_key = format!(
            "policy:{}_escalation",
            match target_role {
                "sales_and_lead" | "discovery_and_proposal" => "commercial",
                "tech_architect_and_coder" | "qa_and_auditor" => "technical",
                "legal_and_compliance" => "legal",
                _ => "manager",
            }
        );

        if let Ok(Some(policy_record)) = self
            .memory_store
            .get_by_key(&MemoryNamespace::Company, &manager_policy_key, None)
        {
            if let Some(threshold) = policy_record
                .content
                .get("threshold_amount")
                .and_then(|v| v.as_f64())
            {
                let amount = payload
                    .get("contract_value_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if amount <= threshold {
                    return Ok(EscalationResolution::Tier2ManagerConsultation {
                        policy_key: manager_policy_key,
                        guidance: format!("Approved within manager threshold limit ${}", threshold),
                    });
                }
            }
        }

        // Tier 3: If external intelligence or missing context is required
        let needs_research = payload
            .get("requires_external_intel")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || target_role == "research_and_intel";

        if needs_research {
            return Ok(EscalationResolution::Tier3WebResearchNeeded {
                query: task_title.to_string(),
                target_role: target_role.to_string(),
            });
        }

        // Tier 4: Founder cryptographic sign-off required
        let digest = cos_contracts::ActionDigest::compute(
            format!("{}:{}:{:?}", task_title, target_role, risk_class).as_bytes(),
        );
        Ok(EscalationResolution::Tier4FounderApprovalRequired {
            action_digest: digest,
            risk_class,
            summary: format!(
                "Founder sign-off mandated for task '{}' (Risk {:?})",
                task_title, risk_class
            ),
        })
    }

    /// Issues a short-lived TaskGrant for an agent to process a specific task.
    pub fn issue_scoped_grant(&self, params: GrantRequestParams) -> TaskGrant {
        TaskGrant {
            grant_id: GrantId::new_v7(),
            task_id: params.task_id,
            agent_id: params.agent_id,
            subject_id: self.executive_identity.subject_id,
            role: params.role,
            allowed_capabilities: params.capabilities,
            client_scope: params.client_scope,
            project_scope: None,
            tenant_scope: self.tenant_id,
            risk_class: params.risk_class,
            approval_reference: None,
            action_digest: params.digest,
            policy_version: params.policy_version,
            grant_status: GrantStatus::Active,
            issued_at: params.now,
            expires_at: params.now + Duration::minutes(params.validity_minutes),
            max_calls: params.max_calls,
            issued_by: self.executive_identity.clone(),
            audit_trace_id: TraceId::new_v7(),
            revocation_reference: None,
        }
    }

    /// Executes a completely generic task by looking up dynamic guidelines in Memory.
    pub fn process_generic_task(
        &self,
        params: ProcessTaskParams,
    ) -> Result<
        (
            TaskRecord,
            Option<ApprovalRequest>,
            cos_contracts::ActionDigest,
        ),
        ExecutiveError,
    > {
        let now = Utc::now();
        let context = TraceContext::root(self.executive_identity.clone());

        // 1. Find or dynamically load assigned agent from Memory
        let agent = self.get_or_load_agent(&params.target_role)?;

        // 2. Create Task Record in Pending -> Assigned -> Planned
        let task_id = TaskId::new_v7();
        let mut task = TaskRecord {
            task_id,
            title: params.task_title.clone(),
            assigned_agent: Some(agent.agent_id),
            status: TaskStatus::Pending,
            created_at: now,
        };

        task.status
            .transition_to(TaskStatus::Assigned)
            .map_err(|e| {
                ExecutiveError::TaskError(format!(
                    "Task assignment state transition failed: {:?}",
                    e
                ))
            })?;
        task.status
            .transition_to(TaskStatus::Planned)
            .map_err(|e| {
                ExecutiveError::TaskError(format!("Task planning state transition failed: {:?}", e))
            })?;

        self.audit_logger
            .record(AuditEvent::new(
                context.clone(),
                AuditEventType::CommandReceived,
                None,
                format!(
                    "Received task '{}' for role '{}'",
                    params.task_title, params.target_role
                ),
            ))
            .ok();

        // 3. Retrieve dynamic guidelines from Memory if requested
        let guideline_record = if let Some(ref key) = params.memory_guideline_key {
            self.memory_store
                .get_by_key(&MemoryNamespace::Company, key, params.client_scope.as_ref())
                .map_err(|e| ExecutiveError::TaskError(format!("Memory query failed: {}", e)))?
        } else {
            None
        };

        // 4. Issue temporary grant to evaluate proposal with dynamic timeouts
        let grant_validity = params
            .custom_grant_duration_minutes
            .unwrap_or(self.governance_config.default_grant_duration_minutes);

        let temp_grant = self.issue_scoped_grant(GrantRequestParams {
            task_id,
            agent_id: agent.agent_id,
            role: agent.role_name.clone(),
            capabilities: vec![params.required_capability.clone()],
            risk_class: params.risk_class,
            digest: cos_contracts::ActionDigest::compute(b"temporary_evaluation_digest"),
            client_scope: params.client_scope,
            validity_minutes: grant_validity,
            max_calls: 1,
            policy_version: self.governance_config.policy_version,
            now,
        });

        // 5. Agent evaluates task using retrieved memory guidelines
        let proposal = agent
            .evaluate_task_with_memory(
                task_id,
                &params.task_payload,
                &temp_grant,
                guideline_record.as_ref(),
                params.target_tool,
                &params.required_capability,
                params.risk_class,
            )
            .map_err(ExecutiveError::TaskError)?;

        // 6. Check for Verified Self-Learning Precedent in Memory:
        // If the Founder previously approved/rejected an identical action digest,
        // we honor the learned policy precedent with 100% confidence.
        let digest_precedent_key = format!("policy:precedent:digest:{}", proposal.action_digest);
        let precedent_opt = self
            .memory_store
            .get_by_key(&MemoryNamespace::Company, &digest_precedent_key, None)
            .ok()
            .flatten();

        let (has_verified_approval_precedent, has_verified_rejection_precedent) =
            if let Some(ref prec) = precedent_opt {
                if prec.lifecycle_status == cos_domain::KnowledgeLifecycle::Verified
                    && prec.confidence >= 0.9
                {
                    let dec = prec
                        .content
                        .get("decision")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    match dec {
                        "approve" | "approved" | "accept" | "yes" => (true, false),
                        "reject" | "rejected" | "deny" | "no" => (false, true),
                        _ => (false, false),
                    }
                } else {
                    (false, false)
                }
            } else {
                (false, false)
            };

        if has_verified_rejection_precedent {
            return Err(ExecutiveError::TaskError(format!(
                "Task '{}' automatically rejected based on permanent Founder precedent policy ({})",
                params.task_title, digest_precedent_key
            )));
        }

        // 7. Check risk class & Memory policy:
        // - R3 and R4 strictly require human approval UNLESS a verified founder approval precedent exists.
        // - R2 requires approval UNLESS dynamic memory guideline explicitly enables routine pre-approved dispatch or precedent exists.
        let is_r2_auto_allowed = if params.risk_class == RiskClass::R2ExternalCommunication {
            guideline_record.as_ref().is_some_and(|r| {
                r.content
                    .get("auto_dispatch_allowed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
        } else {
            false
        };

        let requires_approval = !has_verified_approval_precedent
            && (params.risk_class.requires_human_approval()
                || (params.risk_class == RiskClass::R2ExternalCommunication && !is_r2_auto_allowed));

        if has_verified_approval_precedent {
            self.audit_logger
                .record(AuditEvent::new(
                    context.clone(),
                    AuditEventType::CommandReceived,
                    Some(proposal.action_digest),
                    format!(
                        "Task '{}' auto-approved via verified self-learned precedent from previous Founder sign-off",
                        params.task_title
                    ),
                ))
                .ok();
        }

        let approval = if requires_approval {
            task.status
                .transition_to(TaskStatus::AwaitingApproval)
                .map_err(|e| {
                    ExecutiveError::TaskError(format!(
                        "Task awaiting approval transition failed: {:?}",
                        e
                    ))
                })?;

            let approval_hours = params
                .custom_approval_duration_hours
                .unwrap_or(self.governance_config.default_approval_duration_hours);

            let req = ApprovalRequest::new(
                task_id,
                context.clone(),
                format!("Approve execution of task '{}'", params.task_title),
                proposal.action_digest,
                params.risk_class,
                params.client_scope,
                now + Duration::hours(approval_hours),
            );

            self.audit_logger
                .record(AuditEvent::new(
                    context,
                    AuditEventType::ApprovalRequested,
                    Some(proposal.action_digest),
                    format!("Approval requested for task '{}'", params.task_title),
                ))
                .ok();

            Some(req)
        } else {
            None
        };

        Ok((task, approval, proposal.action_digest))
    }

    /// Dispatches an approved generic task through the Tool Gateway.
    pub fn dispatch_action(
        &self,
        task: &mut TaskRecord,
        params: DispatchActionParams,
    ) -> Result<ToolExecutionResult, ExecutiveError> {
        let now = Utc::now();

        // Handle approval consumption if present
        let approval_id = if let Some(mut app) = params.approval {
            if app.status != cos_domain::ApprovalStatus::Approved {
                let decision_actor = params.approver.as_ref().ok_or_else(|| {
                    ExecutiveError::TaskError(
                        "Approver identity required for approved task".to_string(),
                    )
                })?;

                app.approve(decision_actor.clone(), "Authorized by human reviewer", now)
                    .map_err(|e| ExecutiveError::TaskError(format!("{:?}", e)))?;
            }

            app.consume()
                .map_err(|e| ExecutiveError::TaskError(format!("{:?}", e)))?;

            Some(app.approval_id)
        } else if params.risk_class == RiskClass::R2ExternalCommunication {
            // Policy-approved routine dispatch (cleared by verified memory policy)
            Some(cos_domain::ApprovalId::new_v7())
        } else {
            None
        };

        // Transition task: Executable -> Running
        if task.status == TaskStatus::Planned || task.status == TaskStatus::AwaitingApproval {
            task.status
                .transition_to(TaskStatus::Executable)
                .map_err(|e| {
                    ExecutiveError::TaskError(format!("Task executable transition failed: {:?}", e))
                })?;
        }
        task.status
            .transition_to(TaskStatus::Running)
            .map_err(|e| {
                ExecutiveError::TaskError(format!("Task running transition failed: {:?}", e))
            })?;

        let agent_id = self
            .get_or_load_agent(&params.target_role)
            .map(|a| a.agent_id)
            .unwrap_or_else(|_| AgentId::new_v7());

        // Risk-aware max_calls calculation:
        // - R4 and R3 actions are strictly 1 call (no automatic retries for critical side effects)
        // - Low risk actions use configured default grant calls
        let max_calls = match params.risk_class {
            RiskClass::R4IrreversibleCritical | RiskClass::R3BusinessCommitment => 1,
            _ => self.governance_config.default_grant_max_calls,
        };

        let grant_duration = self.governance_config.default_grant_duration_minutes;

        // Issue final authorized execution grant
        let mut grant = self.issue_scoped_grant(GrantRequestParams {
            task_id: task.task_id,
            agent_id,
            role: params.target_role.clone(),
            capabilities: vec![params.capability],
            risk_class: params.risk_class,
            digest: params.digest,
            client_scope: params.client_scope,
            validity_minutes: grant_duration,
            max_calls,
            policy_version: self.governance_config.policy_version,
            now,
        });
        grant.approval_reference = approval_id;

        let cmd = CommandEnvelope {
            command_id: Uuid::now_v7(),
            command_type: "execute_generic_action".to_string(),
            context: TraceContext::root(self.executive_identity.clone()),
            task_id: Some(task.task_id),
            goal_id: None,
            client_scope: params.client_scope,
            project_scope: None,
            payload: params.payload,
            expected_version: 1,
            capability_grant: Some(grant.clone()),
            approval_reference: approval_id,
            action_digest: params.digest,
            idempotency_key: params.idempotency_key,
            created_at: now,
        };

        let result = self.tool_gateway.execute_command(
            &cmd,
            &grant,
            &params.tool_name,
            params.client_scope.as_ref(),
            params.approver.as_ref(),
        )?;

        // Transition: Running -> Completed
        task.status
            .transition_to(TaskStatus::Completed)
            .map_err(|e| {
                ExecutiveError::TaskError(format!("Task completion transition failed: {:?}", e))
            })?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cos_audit::InMemoryAuditLogger;
    use cos_domain::{ApprovalId, ApprovalStatus, SubjectId};
    use cos_memory::{MemoryKind, MemoryRecord, ProvenanceType};
    use cos_tools::ToolAdapter;

    struct GenericTestTool;

    impl ToolAdapter for GenericTestTool {
        fn tool_name(&self) -> &str {
            "dispatch_service"
        }

        fn capability(&self) -> Capability {
            Capability::new("external_send")
        }

        fn execute(&self, payload: &serde_json::Value) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({ "dispatched": true, "received_payload": payload }))
        }
    }

    #[test]
    fn test_memory_driven_executive_task_flow() {
        let audit_logger = Arc::new(InMemoryAuditLogger::new());
        let memory_store = Arc::new(MemoryStore::new());
        let tool_gateway = Arc::new(ToolGateway::new(audit_logger.clone()));
        tool_gateway.register_tool(Arc::new(GenericTestTool));

        let tenant_id = TenantId::new_v7();
        let executive = CompanyExecutive::new(
            tenant_id,
            audit_logger.clone(),
            memory_store.clone(),
            tool_gateway,
        );

        // 1. Populate Memory with custom business knowledge (No hardcoding in Rust!)
        let qualification_memory = MemoryRecord::new(
            MemoryNamespace::Company,
            MemoryKind::Guideline,
            "onboarding_sop",
            serde_json::json!({
                "rule": "Enterprise onboarding requires dedicated solution architect review",
                "custom_policy_code": "POL_9981"
            }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            ProvenanceType::Learned,
            ActorIdentity::system("founder"),
        );
        memory_store.store(qualification_memory).unwrap();

        // 2. Register dynamic agent with arbitrary role
        let agent = DynamicAgent::new(
            "onboarding_specialist",
            "Coordinates customer onboarding using dynamic SOPs",
            vec![Capability::new("external_send")],
        );
        executive.register_agent(agent);

        // 3. Inbound task arrives with arbitrary JSON data
        let incoming_task_data = serde_json::json!({
            "client_inquiry": "Need architecture audit",
            "tier_level": "VIP"
        });

        let (mut task, approval_opt, digest) = executive
            .process_generic_task(ProcessTaskParams {
                task_title: "Process Customer Inbound".to_string(),
                target_role: "onboarding_specialist".to_string(),
                task_payload: incoming_task_data.clone(),
                memory_guideline_key: Some("onboarding_sop".to_string()),
                target_tool: "dispatch_service".to_string(),
                required_capability: Capability::new("external_send"),
                risk_class: RiskClass::R3BusinessCommitment,
                client_scope: None,
                custom_grant_duration_minutes: None,
                custom_approval_duration_hours: None,
                custom_max_calls: None,
            })
            .expect("Task processing failed");

        assert_eq!(task.status, TaskStatus::AwaitingApproval);
        assert!(approval_opt.is_some());
        let approval = approval_opt.unwrap();

        // 4. Founder approves
        let founder = ActorIdentity::founder(SubjectId::new_v7(), None);
        let exec_result = executive
            .dispatch_action(
                &mut task,
                DispatchActionParams {
                    approval: Some(approval),
                    approver: Some(founder),
                    target_role: "onboarding_specialist".to_string(),
                    tool_name: "dispatch_service".to_string(),
                    capability: Capability::new("external_send"),
                    payload: incoming_task_data,
                    digest,
                    risk_class: RiskClass::R3BusinessCommitment,
                    client_scope: None,
                    idempotency_key: "idem_generic_001".to_string(),
                },
            )
            .expect("Dispatch failed");

        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(exec_result.output["dispatched"], true);
    }

    #[test]
    fn test_r2_auto_dispatch_via_memory_policy() {
        let audit_logger = Arc::new(InMemoryAuditLogger::new());
        let memory_store = Arc::new(MemoryStore::new());
        let tool_gateway = Arc::new(ToolGateway::new(audit_logger.clone()));
        tool_gateway.register_tool(Arc::new(GenericTestTool));

        let tenant_id = TenantId::new_v7();
        let executive =
            CompanyExecutive::new(tenant_id, audit_logger, memory_store.clone(), tool_gateway);

        // Store routine communication guideline with auto_dispatch_allowed: true in Memory
        let routine_policy = MemoryRecord::new(
            MemoryNamespace::Company,
            MemoryKind::Guideline,
            "routine_acknowledgement_policy",
            serde_json::json!({
                "template_name": "standard_receipt",
                "auto_dispatch_allowed": true
            }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            ProvenanceType::Learned,
            ActorIdentity::system("founder"),
        );
        memory_store.store(routine_policy).unwrap();

        let agent = DynamicAgent::new(
            "support_agent",
            "Handles routine customer acknowledgements",
            vec![Capability::new("external_send")],
        );
        executive.register_agent(agent);

        let (mut task, approval_opt, digest) = executive
            .process_generic_task(ProcessTaskParams {
                task_title: "Send Routine Receipt".to_string(),
                target_role: "support_agent".to_string(),
                task_payload: serde_json::json!({ "receipt_id": "REC_101" }),
                memory_guideline_key: Some("routine_acknowledgement_policy".to_string()),
                target_tool: "dispatch_service".to_string(),
                required_capability: Capability::new("external_send"),
                risk_class: RiskClass::R2ExternalCommunication,
                client_scope: None,
                custom_grant_duration_minutes: Some(5),
                custom_approval_duration_hours: None,
                custom_max_calls: Some(2),
            })
            .expect("Task processing failed");

        // Because memory policy allowed auto dispatch, NO human approval was required!
        assert!(approval_opt.is_none());
        assert_ne!(task.status, TaskStatus::AwaitingApproval);

        // Dispatch action directly without human approval
        let result = executive
            .dispatch_action(
                &mut task,
                DispatchActionParams {
                    approval: None,
                    approver: None,
                    target_role: "support_agent".to_string(),
                    tool_name: "dispatch_service".to_string(),
                    capability: Capability::new("external_send"),
                    payload: serde_json::json!({ "receipt_id": "REC_101" }),
                    digest,
                    risk_class: RiskClass::R2ExternalCommunication,
                    client_scope: None,
                    idempotency_key: "idem_receipt_101".to_string(),
                },
            )
            .expect("Direct dispatch failed");

        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(result.output["dispatched"], true);
    }

    #[test]
    fn test_autonomous_engine_tick() {
        let tenant_id = TenantId::new_v7();
        let logger = Arc::new(InMemoryAuditLogger::new());
        let memory = Arc::new(MemoryStore::new());
        let gateway = Arc::new(ToolGateway::new(logger.clone()));

        // Register file_deliverable tool
        let temp_dir = std::env::temp_dir().join(format!("cos_deliv_eng_{}", Uuid::new_v4()));
        gateway.register_tool(Arc::new(cos_tools::FileDeliverableAdapter::new(&temp_dir)));

        let executive = Arc::new(CompanyExecutive::new(
            tenant_id,
            logger.clone(),
            memory.clone(),
            gateway,
        ));

        // Register project operations agent
        let agent = DynamicAgent::new(
            "project_operations",
            "Coordinate task execution",
            vec![Capability::new("filesystem_write")],
        );
        executive.register_agent(agent);

        // Seed an SOP template in memory
        let sop = cos_memory::MemoryRecord::new(
            MemoryNamespace::Company,
            cos_memory::MemoryKind::Procedure,
            "sop:project_operations",
            serde_json::json!({
                "tasks": [
                    {
                        "title": "Generate Sprint Report",
                        "role": "project_operations",
                        "target_tool": "file_deliverable",
                        "required_capability": "filesystem_write",
                        "risk_class": "R1_REVERSIBLE_LOW_IMPACT",
                        "dependencies": [],
                        "payload": {
                            "file_name": "sprint_report.md",
                            "content": "# Sprint Report\nCompleted all operational milestones."
                        }
                    }
                ]
            }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            cos_memory::ProvenanceType::Learned,
            ActorIdentity::system("test_admin"),
        );
        memory.store(sop).unwrap();

        // Create GoalForest with an active goal matching the SOP
        let mut forest_inner = cos_goals::GoalForest::new();
        let mut goal = cos_goals::Goal::new(
            "Project Sprint Coordination",
            "Execute weekly sprint milestones",
        )
        .with_sop_key_hint("sop:project_operations");
        goal.status = cos_goals::GoalStatus::Active;
        goal.criteria.push(cos_goals::SuccessCriterion::new(
            "reports_generated",
            1.0,
            cos_goals::ComparisonOp::GreaterThanOrEqual,
            None,
        ));
        forest_inner.add_goal(goal).unwrap();

        let forest = Arc::new(RwLock::new(forest_inner));
        let learning = Arc::new(cos_learning::LearningEngine::new());

        let engine = AutonomousEngine::new(executive, forest, learning);

        // Run one autonomous engine tick!
        let summary = engine.tick().expect("Autonomous engine tick failed");
        assert_eq!(summary.goals_evaluated, 1);
        assert_eq!(summary.plans_generated, 1);
        assert_eq!(summary.tasks_executed, 1);

        // Verify deliverable was created on disk
        let written_file = temp_dir.join("sprint_report.md");
        assert!(written_file.exists());

        // Clean up temp
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_dynamic_agent_discovery_and_loading_from_memory() {
        let tenant_id = TenantId::new_v7();
        let audit = Arc::new(InMemoryAuditLogger::new());
        let memory = Arc::new(MemoryStore::new());
        let tool_gateway = Arc::new(ToolGateway::new(audit.clone()));

        let executive = CompanyExecutive::new(tenant_id, audit, memory.clone(), tool_gateway);

        // Seed 2 new roles in Memory with zero compile-time code
        let legal_sop = cos_memory::MemoryRecord::new(
            MemoryNamespace::Company,
            cos_memory::MemoryKind::Guideline,
            "sop:legal_and_compliance",
            serde_json::json!({
                "role": "legal_and_compliance",
                "mission": "Review agreements and ensure regulatory compliance",
                "allowed_tools": ["file_deliverable", "notification_sender"],
                "default_tool": "file_deliverable",
                "default_capability": "filesystem_write"
            }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            cos_memory::ProvenanceType::Learned,
            ActorIdentity::system("test_admin"),
        );
        memory.store(legal_sop).unwrap();

        let research_sop = cos_memory::MemoryRecord::new(
            MemoryNamespace::Company,
            cos_memory::MemoryKind::Guideline,
            "sop:research_and_intel",
            serde_json::json!({
                "role": "research_and_intel",
                "mission": "Investigate technical documentation and benchmark solutions",
                "allowed_tools": ["web_research"],
                "default_tool": "web_research",
                "default_capability": "web_research"
            }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            cos_memory::ProvenanceType::Learned,
            ActorIdentity::system("test_admin"),
        );
        memory.store(research_sop).unwrap();

        // Dynamically load all agents from memory
        let count = executive
            .load_all_agents_from_memory()
            .expect("Loading agents failed");
        assert_eq!(count, 2);

        // Verify legal agent was dynamically instantiated with correct capabilities
        let legal_agent = executive
            .get_or_load_agent("legal_and_compliance")
            .expect("Agent should exist");
        assert_eq!(legal_agent.role_name, "legal_and_compliance");
        assert!(legal_agent
            .capabilities
            .contains(&Capability::new("filesystem_write")));
        assert!(legal_agent
            .capabilities
            .contains(&Capability::new("send_notification")));

        // Verify research agent was dynamically instantiated with web_research capability
        let research_agent = executive
            .get_or_load_agent("research_and_intel")
            .expect("Agent should exist");
        assert_eq!(research_agent.role_name, "research_and_intel");
        assert!(research_agent
            .capabilities
            .contains(&Capability::new("web_research")));
    }

    struct InMemoryApprovalQueryStore {
        approvals: RwLock<HashMap<ApprovalId, ApprovalRequest>>,
    }

    impl cos_approval::ApprovalQueryPort for InMemoryApprovalQueryStore {
        fn get_approval(&self, id: &ApprovalId) -> Option<ApprovalRequest> {
            self.approvals.read().unwrap().get(id).cloned()
        }
    }

    #[test]
    fn test_autonomous_engine_approval_sequence_and_resume() {
        let temp_dir = std::env::temp_dir().join(format!("cos-approval-seq-{}", Uuid::new_v4()));
        let tenant_id = TenantId::new_v7();
        let audit = Arc::new(InMemoryAuditLogger::new());
        let memory = Arc::new(MemoryStore::new());
        let tool_gateway = Arc::new(ToolGateway::new(audit.clone()));
        tool_gateway.register_tool(Arc::new(cos_tools::FileDeliverableAdapter::new(&temp_dir)));

        let executive = Arc::new(CompanyExecutive::new(
            tenant_id,
            audit,
            memory.clone(),
            tool_gateway,
        ));

        // Seed Discovery role SOP template in memory with an R3 task that mandates human approval
        let proposal_sop = cos_memory::MemoryRecord::new(
            MemoryNamespace::Company,
            cos_memory::MemoryKind::Procedure,
            "sop:discovery_and_proposal",
            serde_json::json!({
                "tasks": [
                    {
                        "title": "Draft Binding Enterprise Contract",
                        "role": "discovery_and_proposal",
                        "target_tool": "file_deliverable",
                        "required_capability": "filesystem_write",
                        "risk_class": "R3_BUSINESS_COMMITMENT",
                        "dependencies": [],
                        "payload": {
                            "file_name": "enterprise_quote.md",
                            "content": "# Enterprise Quote: $150,000\nBinding commercial commitment."
                        }
                    }
                ]
            }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            cos_memory::ProvenanceType::Learned,
            ActorIdentity::system("test_admin"),
        );
        memory.store(proposal_sop).unwrap();

        let mut forest_inner = cos_goals::GoalForest::new();
        let mut goal =
            cos_goals::Goal::new("Enterprise Proposal", "Draft binding enterprise quote")
                .with_sop_key_hint("sop:discovery_and_proposal");
        goal.status = cos_goals::GoalStatus::Active;
        forest_inner.add_goal(goal).unwrap();

        let forest = Arc::new(RwLock::new(forest_inner));
        let learning = Arc::new(cos_learning::LearningEngine::new());
        let approval_store = Arc::new(InMemoryApprovalQueryStore {
            approvals: RwLock::new(HashMap::new()),
        });

        let engine = AutonomousEngine::new(executive, forest, learning)
            .with_approval_query(approval_store.clone());

        // TICK 1: Task is evaluated -> Requires human approval!
        let summary1 = engine.tick().expect("Tick 1 failed");
        assert_eq!(summary1.tasks_awaiting_approval, 1);
        assert_eq!(summary1.tasks_executed, 0);

        // Verify task was staged in pending_approval_tasks (not duplicated on next tick)
        let pending = engine.pending_approval_tasks.read().unwrap();
        assert_eq!(pending.len(), 1);
        let (_task_id, pending_item) = pending.iter().next().unwrap();
        let app_id = pending_item.approval.approval_id;

        // Store the approval in the query port as Pending
        approval_store
            .approvals
            .write()
            .unwrap()
            .insert(app_id, pending_item.approval.clone());
        drop(pending);

        // TICK 2: While still pending, tick does NOT duplicate or re-request approval!
        let summary2 = engine.tick().expect("Tick 2 failed");
        assert_eq!(summary2.tasks_awaiting_approval, 0);
        assert_eq!(summary2.tasks_executed, 0);

        // Founder approves the request!
        {
            let mut store_lock = approval_store.approvals.write().unwrap();
            let app = store_lock.get_mut(&app_id).unwrap();
            app.status = ApprovalStatus::Approved;
            app.decision_actor = Some(ActorIdentity::founder(SubjectId::new_v7(), None));
        }

        // TICK 3: Engine resumes approved task, executes action, and updates completed tasks!
        let summary3 = engine.tick().expect("Tick 3 failed");
        assert_eq!(summary3.tasks_executed, 1);
        assert_eq!(engine.completed_tasks.read().unwrap().len(), 1);
        assert_eq!(engine.pending_approval_tasks.read().unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_self_learning_precedent_auto_approval() {
        let audit_logger = Arc::new(InMemoryAuditLogger::new());
        let memory_store = Arc::new(MemoryStore::new());
        let tool_gateway = Arc::new(ToolGateway::new(audit_logger.clone()));
        tool_gateway.register_tool(Arc::new(GenericTestTool));

        let tenant_id = TenantId::new_v7();
        let executive =
            CompanyExecutive::new(tenant_id, audit_logger, memory_store.clone(), tool_gateway);

        // Register specialist agent
        executive.register_agent(DynamicAgent::new(
            "tech_architect_and_coder",
            "Designs architecture and writes modular code",
            vec![Capability::new("code_execution")],
        ));

        let task_params = ProcessTaskParams {
            task_title: "Deploy high-risk infrastructure module".to_string(),
            target_role: "tech_architect_and_coder".to_string(),
            task_payload: serde_json::json!({ "module": "auth_core" }),
            memory_guideline_key: None,
            target_tool: "dispatch_service".to_string(),
            required_capability: Capability::new("filesystem_write"),
            risk_class: RiskClass::R3BusinessCommitment,
            client_scope: None,
            custom_grant_duration_minutes: None,
            custom_approval_duration_hours: None,
            custom_max_calls: None,
        };

        // First execution: R3 strictly requires human approval
        let (task1, approval1, digest1) = executive
            .process_generic_task(task_params.clone())
            .expect("First evaluation failed");
        assert!(approval1.is_some(), "R3 task must initially require approval");
        assert_eq!(task1.status, TaskStatus::AwaitingApproval);

        // Founder approves the task -> Permanent Precedent indexed by digest is saved
        let precedent = cos_memory::MemoryRecord::new(
            MemoryNamespace::Company,
            cos_memory::MemoryKind::Guideline,
            format!("policy:precedent:digest:{}", digest1),
            serde_json::json!({
                "decision": "approve",
                "action_digest": digest1,
                "learned_rule": "Auto-approved auth_core deployment precedent"
            }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            cos_memory::ProvenanceType::Learned,
            ActorIdentity::founder(SubjectId::new_v7(), None),
        );
        memory_store.store(precedent).unwrap();

        // Second execution: System detects verified precedent and auto-approves!
        let (task2, approval2, _digest2) = executive
            .process_generic_task(task_params)
            .expect("Second evaluation failed");
        assert!(
            approval2.is_none(),
            "Self-learned precedent must auto-approve without generating ApprovalRequest"
        );
        assert_eq!(task2.status, TaskStatus::Planned);
    }

    #[test]
    fn test_four_tier_escalation_ladder() {
        let audit_logger = Arc::new(InMemoryAuditLogger::new());
        let memory_store = Arc::new(MemoryStore::new());
        let tool_gateway = Arc::new(ToolGateway::new(audit_logger.clone()));

        // Seed Tier 2 manager escalation policy with threshold limit $10,000
        let manager_policy = cos_memory::MemoryRecord::new(
            MemoryNamespace::Company,
            cos_memory::MemoryKind::Guideline,
            "policy:commercial_escalation".to_string(),
            serde_json::json!({
                "threshold_amount": 10000.0,
                "domain": "commercial"
            }),
            cos_domain::KnowledgeLifecycle::Verified,
            1.0,
            cos_memory::ProvenanceType::Learned,
            ActorIdentity::founder(SubjectId::new_v7(), None),
        );
        memory_store.store(manager_policy).unwrap();

        let tenant_id = TenantId::new_v7();
        let executive =
            CompanyExecutive::new(tenant_id, audit_logger, memory_store, tool_gateway);

        // Tier 1: Low risk internal task
        let res1 = executive
            .evaluate_escalation_ladder(
                "Internal refactoring",
                "tech_architect_and_coder",
                RiskClass::R0InternalRead,
                &serde_json::json!({}),
            )
            .unwrap();
        assert!(matches!(res1, EscalationResolution::Tier1Autonomous { .. }));

        // Tier 2: Commercial contract within manager policy threshold ($5,000 <= $10,000)
        let res2 = executive
            .evaluate_escalation_ladder(
                "Sign small client proposal",
                "sales_and_lead",
                RiskClass::R2ExternalCommunication,
                &serde_json::json!({ "contract_value_usd": 5000.0 }),
            )
            .unwrap();
        assert!(matches!(res2, EscalationResolution::Tier2ManagerConsultation { .. }));

        // Tier 3: Requires external intelligence
        let res3 = executive
            .evaluate_escalation_ladder(
                "Research market competitor pricing",
                "research_and_intel",
                RiskClass::R2ExternalCommunication,
                &serde_json::json!({ "requires_external_intel": true }),
            )
            .unwrap();
        assert!(matches!(res3, EscalationResolution::Tier3WebResearchNeeded { .. }));

        // Tier 4: Exceeds threshold or irreversible action without precedent -> Founder signoff
        let res4 = executive
            .evaluate_escalation_ladder(
                "Sign mega contract",
                "sales_and_lead",
                RiskClass::R4IrreversibleCritical,
                &serde_json::json!({ "contract_value_usd": 500000.0 }),
            )
            .unwrap();
        assert!(matches!(res4, EscalationResolution::Tier4FounderApprovalRequired { .. }));
    }
}
