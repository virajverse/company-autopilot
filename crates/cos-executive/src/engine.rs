use cos_contracts::Capability;
use cos_domain::TaskId;
use cos_goals::{AnomalyDetector, GoalForest, GoalId, GoalStatus, SelfGoalGovernor};
use cos_learning::{LearningEngine, LearningProposal};
use cos_memory::MemoryNamespace;
use cos_outcomes::{OutcomeRecord, OutcomeRegistry, OutcomeStatus};
use cos_planning::{DynamicPlanner, ExecutionPlan};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

use crate::{CompanyExecutive, DispatchActionParams, ProcessTaskParams};

/// Summary of activity performed during a single autonomous engine tick.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
        }
    }

    /// Performs one discrete pass of the autonomous executive loop.
    pub fn tick(&self) -> Result<TickSummary, Box<dyn std::error::Error>> {
        let mut summary = TickSummary::default();

        // 1. Gather all active goals from the GoalForest
        let active_goals = {
            let forest = self.forest.read().unwrap();
            forest
                .iter()
                .filter(|g| g.status == GoalStatus::Active)
                .cloned()
                .collect::<Vec<_>>()
        };

        summary.goals_evaluated = active_goals.len();

        // 2. Goal Decomposition: ensure every active goal has an ExecutionPlan
        for goal in &active_goals {
            let has_plan = {
                let plans = self.plans.read().unwrap();
                plans.contains_key(&goal.id)
            };

            if !has_plan {
                // Decompose goal using dynamic memory SOP templates
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

        // 3. Task Execution: find and execute ready tasks in active plans
        let completed_snapshot = self.completed_tasks.read().unwrap().clone();
        let plan_goal_ids: Vec<GoalId> = self.plans.read().unwrap().keys().cloned().collect();

        for goal_id in plan_goal_ids {
            let ready_tasks = {
                let plans = self.plans.read().unwrap();
                if let Some(plan) = plans.get(&goal_id) {
                    plan.get_ready_tasks(&completed_snapshot)
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            };

            for task in ready_tasks {
                // Determine tool and capability based on task or defaults
                let (target_tool, required_capability) = match task.required_role.as_str() {
                    "sales_and_lead" | "sales" => (
                        "notification_sender".to_string(),
                        Capability::new("send_notification"),
                    ),
                    "discovery_and_proposal" | "discovery" => (
                        "file_deliverable".to_string(),
                        Capability::new("filesystem_write"),
                    ),
                    "client_success_communication" | "client_success" => (
                        "notification_sender".to_string(),
                        Capability::new("send_notification"),
                    ),
                    _ => (
                        "file_deliverable".to_string(),
                        Capability::new("filesystem_write"),
                    ),
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
                        if approval.is_some() {
                            // High risk or non-preapproved R2: waiting for human approval
                            summary.tasks_awaiting_approval += 1;
                            debug!(
                                "Task '{}' requires human signoff before execution",
                                task.title
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

        // 4. Operational Anomaly Evaluation (Phase 9)
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
