use chrono::Utc;
use cos_approval::{ApprovalQueryPort, ApprovalRequest};
use cos_contracts::{ActionDigest, Capability};
use cos_domain::{ApprovalStatus, RiskClass, TaskId, TaskStatus};
use cos_goals::{AnomalyDetector, GoalForest, GoalId, GoalStatus, SelfGoalGovernor};
use cos_learning::{LearningEngine, LearningProposal};
use cos_memory::MemoryNamespace;
use cos_outcomes::{OutcomeRecord, OutcomeRegistry, OutcomeStatus};
use cos_planning::{DynamicPlanner, ExecutionPlan};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

use crate::{CompanyExecutive, DispatchActionParams, ProcessTaskParams, TaskRecord};

/// Execution state preserved while a high-risk or policy-mandated task awaits human approval.
#[derive(Debug, Clone)]
pub struct PendingTaskExecution {
    pub goal_id: GoalId,
    pub task_id: TaskId,
    pub title: String,
    pub required_role: String,
    pub target_tool: String,
    pub required_capability: Capability,
    pub input_payload: serde_json::Value,
    pub approval: ApprovalRequest,
    pub digest: ActionDigest,
    pub risk_class: RiskClass,
}

/// Summary of activity performed during a single autonomous engine tick.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TickSummary {
    pub goals_evaluated: usize,
    pub plans_generated: usize,
    pub tasks_executed: usize,
    pub tasks_awaiting_approval: usize,
    pub anomalies_detected: usize,
}

/// The Autonomous Engine coordinates the continuous operational loop of the company:
///
/// `GOAL -> PLAN -> AGENT -> EXECUTIVE -> APPROVAL -> TOOL -> OUTCOME -> LEARNING -> MEMORY`
///
/// It scans active business goals, decomposes them using dynamic memory templates,
/// executes ready tasks through the Executive authority boundary, monitors for anomalies,
/// and updates goal progress autonomously.
pub struct AutonomousEngine {
    pub executive: Arc<CompanyExecutive>,
    pub forest: Arc<RwLock<GoalForest>>,
    pub planner: DynamicPlanner,
    pub learning: Arc<LearningEngine>,
    pub detector: AnomalyDetector,
    pub governor: SelfGoalGovernor,
    pub outcomes: Arc<OutcomeRegistry>,
    pub plans: RwLock<HashMap<GoalId, ExecutionPlan>>,
    pub completed_tasks: RwLock<HashSet<TaskId>>,
    pub pending_approval_tasks: RwLock<HashMap<TaskId, PendingTaskExecution>>,
    pub approval_query: Option<Arc<dyn ApprovalQueryPort>>,
}

impl AutonomousEngine {
    pub fn new(
        executive: Arc<CompanyExecutive>,
        forest: Arc<RwLock<GoalForest>>,
        learning: Arc<LearningEngine>,
    ) -> Self {
        Self {
            executive,
            forest,
            planner: DynamicPlanner::new(),
            learning,
            detector: AnomalyDetector::new(),
            governor: SelfGoalGovernor::default(),
            outcomes: Arc::new(OutcomeRegistry::new()),
            plans: RwLock::new(HashMap::new()),
            completed_tasks: RwLock::new(HashSet::new()),
            pending_approval_tasks: RwLock::new(HashMap::new()),
            approval_query: None,
        }
    }

    /// Attaches an approval query port so the engine automatically detects and resumes
    /// tasks after human approval decisions without requiring polling or external intervention.
    pub fn with_approval_query(mut self, port: Arc<dyn ApprovalQueryPort>) -> Self {
        self.approval_query = Some(port);
        self
    }

    /// Returns the current runtime execution plans for active goals.
    pub fn get_plans(&self) -> HashMap<GoalId, ExecutionPlan> {
        self.plans.read().unwrap().clone()
    }

    /// Returns the set of completed task IDs.
    pub fn get_completed_tasks(&self) -> HashSet<TaskId> {
        self.completed_tasks.read().unwrap().clone()
    }

    /// Returns the currently pending task executions awaiting human decision.
    pub fn get_pending_tasks(&self) -> Vec<PendingTaskExecution> {
        self.pending_approval_tasks
            .read()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    /// Returns high-level engine status summary for monitoring and dashboards.
    pub fn get_status_summary(&self) -> serde_json::Value {
        let plans = self.plans.read().unwrap();
        let completed = self.completed_tasks.read().unwrap();
        let pending = self.pending_approval_tasks.read().unwrap();

        serde_json::json!({
            "active_plans_count": plans.len(),
            "completed_tasks_count": completed.len(),
            "pending_approvals_count": pending.len(),
            "pending_tasks": pending
                .values()
                .map(|p| {
                    serde_json::json!({
                        "task_id": p.task_id,
                        "goal_id": p.goal_id,
                        "title": p.title,
                        "required_role": p.required_role,
                        "risk_class": p.risk_class,
                        "approval_id": p.approval.approval_id,
                    })
                })
                .collect::<Vec<_>>()
        })
    }

    /// Performs one discrete pass of the autonomous executive loop in strict causal sequence:
    ///
    /// 1. RESUME APPROVED TASKS: Checks pending approvals and dispatches tasks approved by human reviewer.
    /// 2. GOAL DECOMPOSITION: Decomposes active goals into DAG execution plans using Memory SOPs.
    /// 3. TASK EXECUTION: Dispatches ready tasks, preserving strict DAG order and preventing duplicates.
    /// 4. ANOMALY EVALUATION: Detects deviations and injects autonomous remediation goals.
    pub fn tick(&self) -> Result<TickSummary, Box<dyn std::error::Error>> {
        let mut summary = TickSummary::default();

        // PHASE 1: Resume / Unblock tasks that have received human approval
        if let Some(ref query) = self.approval_query {
            let pending_list: Vec<(TaskId, PendingTaskExecution)> = {
                let pending = self.pending_approval_tasks.read().unwrap();
                pending.iter().map(|(k, v)| (*k, v.clone())).collect()
            };

            for (task_id, pending_item) in pending_list {
                if let Some(app) = query.get_approval(&pending_item.approval.approval_id) {
                    match app.status {
                        ApprovalStatus::Approved => {
                            info!(
                                "Approval confirmed by {:?} for task '{}'! Dispatching execution...",
                                app.decision_actor, pending_item.title
                            );
                            let mut task_record = TaskRecord {
                                task_id,
                                title: pending_item.title.clone(),
                                assigned_agent: None,
                                status: TaskStatus::AwaitingApproval,
                                created_at: Utc::now(),
                            };

                            let dispatch_params = DispatchActionParams {
                                approval: Some(app.clone()),
                                approver: app.decision_actor.clone(),
                                target_role: pending_item.required_role.clone(),
                                tool_name: pending_item.target_tool.clone(),
                                capability: pending_item.required_capability.clone(),
                                payload: pending_item.input_payload.clone(),
                                digest: pending_item.digest,
                                risk_class: pending_item.risk_class,
                                client_scope: None,
                                idempotency_key: format!("auto_{}", task_id),
                            };

                            match self
                                .executive
                                .dispatch_action(&mut task_record, dispatch_params)
                            {
                                Ok(_) => {
                                    self.completed_tasks.write().unwrap().insert(task_id);
                                    self.pending_approval_tasks
                                        .write()
                                        .unwrap()
                                        .remove(&task_id);
                                    summary.tasks_executed += 1;

                                    let outcome = OutcomeRecord::new(
                                        task_id,
                                        OutcomeStatus::Success,
                                        format!(
                                            "Task '{}' executed successfully following human approval",
                                            pending_item.title
                                        ),
                                        1.0,
                                        self.executive.executive_identity.clone(),
                                    );
                                    let _ = self.outcomes.record(outcome);

                                    let mut forest = self.forest.write().unwrap();
                                    if let Some(first_crit) = forest
                                        .get_goal(&pending_item.goal_id)
                                        .and_then(|g| g.criteria.first())
                                        .map(|c| c.name.clone())
                                    {
                                        let _ = forest.update_criterion(
                                            &pending_item.goal_id,
                                            &first_crit,
                                            1.0,
                                        );
                                    }
                                }
                                Err(err) => {
                                    warn!(
                                        "Approved action dispatch failed for '{}': {}",
                                        pending_item.title, err
                                    );
                                }
                            }
                        }
                        ApprovalStatus::Rejected => {
                            info!(
                                "Approval rejected for task '{}'. Halting branch.",
                                pending_item.title
                            );
                            self.pending_approval_tasks
                                        .write()
                                        .unwrap()
                                        .remove(&task_id);
                            let outcome = OutcomeRecord::new(
                                task_id,
                                OutcomeStatus::Failure,
                                format!(
                                    "Task '{}' rejected by human approver",
                                    pending_item.title
                                ),
                                0.0,
                                self.executive.executive_identity.clone(),
                            );
                            let _ = self.outcomes.record(outcome);
                        }
                        _ => {} // Still awaiting decision or expired
                    }
                }
            }
        }

        // PHASE 2: Gather all active goals from the GoalForest
        let active_goals = {
            let forest = self.forest.read().unwrap();
            forest
                .iter()
                .filter(|g| g.status == GoalStatus::Active)
                .cloned()
                .collect::<Vec<_>>()
        };

        summary.goals_evaluated = active_goals.len();

        // Ensure every active goal has an ExecutionPlan
        for goal in &active_goals {
            let has_plan = {
                let plans = self.plans.read().unwrap();
                plans.contains_key(&goal.id)
            };

            if !has_plan {
                match self
                    .planner
                    .plan_from_goal(goal, &self.executive.memory_store)
                {
                    Ok(plan) => {
                        debug!(
                            "Decomposed goal '{}' into {} planned tasks",
                            goal.title,
                            plan.tasks.len()
                        );
                        self.plans.write().unwrap().insert(goal.id, plan);
                        summary.plans_generated += 1;
                    }
                    Err(err) => {
                        warn!("Failed to plan goal '{}': {}", goal.title, err);
                    }
                }
            }
        }

        // PHASE 3: Task Execution in Strict DAG Order (Filtered for Concurrency & Zero Duplicates)
        let completed_snapshot = self.completed_tasks.read().unwrap().clone();
        let pending_snapshot: HashSet<TaskId> = self
            .pending_approval_tasks
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect();

        let plan_goal_ids: Vec<GoalId> = self.plans.read().unwrap().keys().cloned().collect();

        for goal_id in plan_goal_ids {
            let ready_tasks = {
                let plans = self.plans.read().unwrap();
                if let Some(plan) = plans.get(&goal_id) {
                    plan.get_ready_tasks(&completed_snapshot)
                        .into_iter()
                        .filter(|t| !pending_snapshot.contains(&t.id))
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            };

            for task in ready_tasks {
                // Dynamically resolve target tool & capability from the role's SOP in Memory (Zero Hardcoded If/Else)
                let guideline_key = format!("sop:{}", task.required_role);
                let sop_record = self
                    .executive
                    .memory_store
                    .get_by_key(&MemoryNamespace::Company, &guideline_key, None)
                    .ok()
                    .flatten();

                let (target_tool, required_capability) = if let Some(ref r) = sop_record {
                    let tool = r
                        .content
                        .get("default_tool")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            r.content
                                .get("allowed_tools")
                                .and_then(|v| v.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|v| v.as_str())
                        })
                        .unwrap_or("file_deliverable")
                        .to_string();

                    let default_cap = match tool.as_str() {
                        "notification_sender" => "send_notification",
                        "http_webhook" => "network_http",
                        "web_research" => "web_research",
                        _ => "filesystem_write",
                    };

                    let cap_str = r
                        .content
                        .get("default_capability")
                        .and_then(|v| v.as_str())
                        .unwrap_or(default_cap);

                    (tool, Capability::new(cap_str))
                } else {
                    (
                        "file_deliverable".to_string(),
                        Capability::new("filesystem_write"),
                    )
                };

                let params = ProcessTaskParams {
                    task_title: task.title.clone(),
                    target_role: task.required_role.clone(),
                    task_payload: task.input_payload.clone(),
                    memory_guideline_key: Some(format!("sop:{}", task.required_role)),
                    target_tool: target_tool.clone(),
                    required_capability: required_capability.clone(),
                    risk_class: task.estimated_risk,
                    client_scope: None,
                    custom_grant_duration_minutes: None,
                    custom_approval_duration_hours: None,
                    custom_max_calls: None,
                };

                match self.executive.process_generic_task(params) {
                    Ok((task_record, approval, digest)) => {
                        if let Some(req) = approval {
                            summary.tasks_awaiting_approval += 1;
                            debug!(
                                "Task '{}' requires human signoff before execution",
                                task.title
                            );
                            self.pending_approval_tasks.write().unwrap().insert(
                                task.id,
                                PendingTaskExecution {
                                    goal_id,
                                    task_id: task.id,
                                    title: task.title.clone(),
                                    required_role: task.required_role.clone(),
                                    target_tool: target_tool.clone(),
                                    required_capability: required_capability.clone(),
                                    input_payload: task.input_payload.clone(),
                                    approval: req,
                                    digest,
                                    risk_class: task.estimated_risk,
                                },
                            );
                        } else {
                            // Low risk (R0, R1) or pre-approved R2: immediately dispatch
                            let dispatch_params = DispatchActionParams {
                                approval: None,
                                approver: None,
                                target_role: task.required_role.clone(),
                                tool_name: target_tool,
                                capability: required_capability,
                                payload: task.input_payload.clone(),
                                digest,
                                risk_class: task.estimated_risk,
                                client_scope: None,
                                idempotency_key: format!("auto_{}", task_record.task_id),
                            };

                            let mut task_record = task_record;
                            match self
                                .executive
                                .dispatch_action(&mut task_record, dispatch_params)
                            {
                                Ok(_tool_result) => {
                                    self.completed_tasks.write().unwrap().insert(task.id);
                                    summary.tasks_executed += 1;

                                    // Record successful outcome
                                    let outcome = OutcomeRecord::new(
                                        task_record.task_id,
                                        OutcomeStatus::Success,
                                        format!(
                                            "Task '{}' successfully executed autonomously",
                                            task.title
                                        ),
                                        1.0,
                                        self.executive.executive_identity.clone(),
                                    );
                                    let _ = self.outcomes.record(outcome);

                                    // Bubble progress up to Goal criteria if applicable
                                    let mut forest = self.forest.write().unwrap();
                                    if let Some(first_crit) = forest
                                        .get_goal(&goal_id)
                                        .and_then(|g| g.criteria.first())
                                        .map(|c| c.name.clone())
                                    {
                                        let _ = forest.update_criterion(&goal_id, &first_crit, 1.0);
                                    }
                                }
                                Err(err) => {
                                    warn!(
                                        "Tool dispatch failed for task '{}': {}",
                                        task.title, err
                                    );
                                    // Submit learning proposal on failure
                                    let target_key = format!("sop:{}", task.required_role);
                                    if let Ok(prop) = LearningProposal::new(
                                        MemoryNamespace::Company,
                                        &target_key,
                                        format!("Autonomous tool dispatch error: {}", err),
                                        serde_json::json!({
                                            "role": task.required_role,
                                            "failure_note": err.to_string(),
                                            "remediation": "review tool parameters and risk policy"
                                        }),
                                        Vec::new(),
                                    ) {
                                        self.learning.submit_proposal(prop);
                                    }
                                }
                            }
                        }
                    }
                    Err(err) => {
                        warn!(
                            "Executive processing failed for task '{}': {}",
                            task.title, err
                        );
                    }
                }
            }
        }

        // PHASE 4: Operational Anomaly Evaluation (Self-Healing Governance)
        for goal in &active_goals {
            if let Some(anomaly) = self.detector.evaluate_goal(goal) {
                summary.anomalies_detected += 1;
                let candidate = self.governor.generate_remediation_goal(&anomaly, goal.id);
                if self.governor.validate_candidate_goal(&candidate, 1).is_ok() {
                    let mut forest = self.forest.write().unwrap();
                    let _ = forest.add_goal(candidate);
                }
            }
        }

        Ok(summary)
    }

    /// Spawns the autonomous heartbeat daemon in a background async task.
    pub async fn run_daemon(
        self: Arc<Self>,
        interval: std::time::Duration,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) {
        info!(
            "Autonomous Executive loop started (interval: {:?})",
            interval
        );
        let mut ticker = tokio::time::interval(interval);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match self.tick() {
                        Ok(summary) => {
                            if summary.tasks_executed > 0 || summary.tasks_awaiting_approval > 0 || summary.plans_generated > 0 {
                                info!("Autonomous Engine Tick: {:?}", summary);
                            }
                        }
                        Err(e) => {
                            warn!("Autonomous Engine Tick error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Autonomous Executive loop received shutdown signal. Terminating gracefully.");
                    break;
                }
            }
        }
    }
}
