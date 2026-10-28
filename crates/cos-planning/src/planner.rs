use crate::types::{ExecutionPlan, PlanStatus, PlannedTask, PlanningError};
use cos_domain::{RiskClass, TaskId};
use cos_goals::Goal;
use cos_memory::{MemoryKind, MemoryNamespace, MemoryRecord, MemoryStore};
use std::collections::HashSet;

/// Parses RiskClass flexibly from case-insensitive string or numeric code.
fn parse_risk_class(val: Option<&serde_json::Value>) -> RiskClass {
    let raw = match val {
        Some(serde_json::Value::String(s)) => s.to_lowercase(),
        Some(serde_json::Value::Number(n)) => format!("r{}", n),
        _ => return RiskClass::R1ReversibleLowImpact,
    };

    if raw.contains("r4") || raw.contains("critical") || raw.contains("founder") {
        RiskClass::R4IrreversibleCritical
    } else if raw.contains("r3") || raw.contains("commitment") || raw.contains("business") {
        RiskClass::R3BusinessCommitment
    } else if raw.contains("r2") || raw.contains("external") || raw.contains("comm") {
        RiskClass::R2ExternalCommunication
    } else if raw.contains("r0") || raw.contains("read") {
        RiskClass::R0InternalRead
    } else {
        RiskClass::R1ReversibleLowImpact
    }
}

/// Matches the most relevant SOP record for a goal using multi-criteria matching.
fn find_matching_sop<'a>(goal: &Goal, procedures: &'a [MemoryRecord]) -> Option<&'a MemoryRecord> {
    // 1. Explicit SOP hint in Goal
    if let Some(hint) = &goal.sop_key_hint {
        if let Some(matched) = procedures
            .iter()
            .find(|rec| rec.key.eq_ignore_ascii_case(hint))
        {
            return Some(matched);
        }
    }

    // 2. Tag-based intersection
    if !goal.tags.is_empty() {
        for rec in procedures {
            if let Some(tags_val) = rec.content.get("tags").and_then(|t| t.as_array()) {
                let rec_tags: Vec<String> = tags_val
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect();

                let has_overlap = goal.tags.iter().any(|gt| {
                    rec_tags.contains(&gt.to_lowercase())
                        || rec.key.to_lowercase().contains(&gt.to_lowercase())
                });

                if has_overlap {
                    return Some(rec);
                }
            }
        }
    }

    // 3. Word token overlap between goal title and SOP key/title
    let goal_words: HashSet<String> = goal
        .title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3)
        .map(|w| w.to_string())
        .collect();

    let mut best_match: Option<(&'a MemoryRecord, usize)> = None;

    for rec in procedures {
        let key_words: HashSet<String> = rec
            .key
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .map(|w| w.to_string())
            .collect();

        let overlap = goal_words.intersection(&key_words).count();
        if overlap > 0 {
            match best_match {
                Some((_, current_max)) if overlap > current_max => {
                    best_match = Some((rec, overlap));
                }
                None => {
                    best_match = Some((rec, overlap));
                }
                _ => {}
            }
        }
    }

    best_match.map(|(rec, _)| rec)
}

/// Planning engine responsible for decomposing high-level business goals into executable task DAGs.
#[derive(Debug, Clone, Default)]
pub struct DynamicPlanner;

impl DynamicPlanner {
    pub fn new() -> Self {
        Self
    }

    /// Decomposes an active goal into an ExecutionPlan using dynamically stored SOP guidelines from Memory.
    pub fn plan_from_goal(
        &self,
        goal: &Goal,
        memory_store: &MemoryStore,
    ) -> Result<ExecutionPlan, PlanningError> {
        // Query memory store for company-wide procedures associated with this goal
        let procedures = memory_store
            .query_records(&MemoryNamespace::Company, Some(MemoryKind::Procedure), None)
            .unwrap_or_default();

        let mut planned_tasks = Vec::new();

        // 1. Dynamic SOP matching (Hints -> Tags -> Token overlap)
        let matching_sop = find_matching_sop(goal, &procedures);

        if let Some(sop) = matching_sop {
            let steps_opt = sop
                .content
                .get("steps")
                .or_else(|| sop.content.get("tasks"))
                .and_then(|s| s.as_array());

            if let Some(steps) = steps_opt {
                let mut prev_task_id: Option<TaskId> = None;
                for step in steps {
                    let title = step.get("title").and_then(|v| v.as_str()).unwrap_or("Task");
                    let desc = step
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let role = step
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("default_worker");
                    let risk =
                        parse_risk_class(step.get("risk").or_else(|| step.get("risk_class")));

                    let mut task = PlannedTask::new(goal.id, title, desc, role, risk);
                    if let Some(payload) = step.get("payload") {
                        task = task.with_payload(payload.clone());
                    }
                    if let Some(prev) = prev_task_id {
                        task = task.with_prerequisite(prev);
                    }
                    let current_id = task.id;
                    planned_tasks.push(task);
                    prev_task_id = Some(current_id);
                }
            }
        }

        // 2. Dynamic Fallback from Memory: Check if a dynamic template exists in memory
        if planned_tasks.is_empty() {
            let default_template = memory_store
                .get_by_key(&MemoryNamespace::Company, "default_planning_template", None)
                .ok()
                .flatten()
                .or_else(|| {
                    memory_store
                        .get_by_key(
                            &MemoryNamespace::Company,
                            "sop:default_planning_template",
                            None,
                        )
                        .ok()
                        .flatten()
                });

            if let Some(tpl) = default_template {
                let steps_opt = tpl
                    .content
                    .get("steps")
                    .or_else(|| tpl.content.get("tasks"))
                    .and_then(|s| s.as_array());

                if let Some(steps) = steps_opt {
                    let mut prev_task_id: Option<TaskId> = None;
                    for step in steps {
                        let title_suffix =
                            step.get("stage").and_then(|v| v.as_str()).unwrap_or("Step");
                        let title = format!("{}: {}", goal.title, title_suffix);
                        let desc = step
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let role = step
                            .get("role")
                            .and_then(|v| v.as_str())
                            .unwrap_or("default_worker");
                        let risk =
                            parse_risk_class(step.get("risk").or_else(|| step.get("risk_class")));

                        let mut task = PlannedTask::new(goal.id, title, desc, role, risk);
                        if let Some(payload) = step.get("payload") {
                            task = task.with_payload(payload.clone());
                        }
                        if let Some(prev) = prev_task_id {
                            task = task.with_prerequisite(prev);
                        }
                        let current_id = task.id;
                        planned_tasks.push(task);
                        prev_task_id = Some(current_id);
                    }
                }
            }
        }

        // 3. Minimal bootstrap fallback if memory is entirely blank
        if planned_tasks.is_empty() {
            let task1 = PlannedTask::new(
                goal.id,
                format!("{}: Preparation", goal.title),
                format!("Prepare execution for: {}", goal.description),
                "default_worker",
                RiskClass::R1ReversibleLowImpact,
            );
            let t1_id = task1.id;

            let task2 = PlannedTask::new(
                goal.id,
                format!("{}: Execution", goal.title),
                format!("Execute operational phase for: {}", goal.description),
                "default_worker",
                RiskClass::R2ExternalCommunication,
            )
            .with_prerequisite(t1_id);

            planned_tasks.push(task1);
            planned_tasks.push(task2);
        }

        ExecutionPlan::new(goal.id, format!("Plan for: {}", goal.title), planned_tasks)
    }

    /// Evaluates which tasks are ready given a set of completed task IDs.
    pub fn ready_tasks<'a>(
        &self,
        plan: &'a ExecutionPlan,
        completed: &HashSet<TaskId>,
    ) -> Vec<&'a PlannedTask> {
        plan.get_ready_tasks(completed)
    }

    /// Updates execution plan status based on completed tasks.
    pub fn evaluate_plan_status(
        &self,
        plan: &mut ExecutionPlan,
        completed: &HashSet<TaskId>,
    ) -> PlanStatus {
        let all_completed = plan.tasks.iter().all(|t| completed.contains(&t.id));
        if all_completed {
            plan.status = PlanStatus::Completed;
        } else if !completed.is_empty() {
            plan.status = PlanStatus::Executing;
        }
        plan.status
    }
}
