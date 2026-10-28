use chrono::{DateTime, Utc};
use cos_domain::{RiskClass, TaskId};
use cos_goals::GoalId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PlanningError {
    #[error("Cyclic dependency detected among plan tasks")]
    CyclicDependency,

    #[error("Task '{0}' references unknown prerequisite task '{1}'")]
    MissingPrerequisite(TaskId, TaskId),

    #[error("Empty plan cannot be executed")]
    EmptyPlan,

    #[error("Task '{0}' not found in execution plan")]
    TaskNotFound(TaskId),
}

/// Lifecycle status of an operational execution plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    #[default]
    Draft,
    Ready,
    Executing,
    Completed,
    Failed,
}

/// An individual decomposed task within an execution plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedTask {
    pub id: TaskId,
    pub goal_id: GoalId,
    pub title: String,
    pub description: String,
    /// Generic dynamic role capable of processing this task (e.g. "operations-assistant").
    pub required_role: String,
    /// Estimated risk class governing approval requirements.
    pub estimated_risk: RiskClass,
    /// Task IDs that must successfully complete before this task can execute.
    pub prerequisite_task_ids: Vec<TaskId>,
    /// Dynamic JSON payload for task execution.
    pub input_payload: serde_json::Value,
}

impl PlannedTask {
    pub fn new(
        goal_id: GoalId,
        title: impl Into<String>,
        description: impl Into<String>,
        required_role: impl Into<String>,
        estimated_risk: RiskClass,
    ) -> Self {
        Self {
            id: TaskId::new_v7(),
            goal_id,
            title: title.into(),
            description: description.into(),
            required_role: required_role.into(),
            estimated_risk,
            prerequisite_task_ids: Vec::new(),
            input_payload: serde_json::json!({}),
        }
    }

    pub fn with_prerequisite(mut self, prereq_id: TaskId) -> Self {
        self.prerequisite_task_ids.push(prereq_id);
        self
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.input_payload = payload;
        self
    }
}

/// A validated DAG of planned tasks achieving a specific goal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub id: Uuid,
    pub goal_id: GoalId,
    pub title: String,
    pub status: PlanStatus,
    pub tasks: Vec<PlannedTask>,
    pub created_at: DateTime<Utc>,
}

impl ExecutionPlan {
    pub fn new(
        goal_id: GoalId,
        title: impl Into<String>,
        tasks: Vec<PlannedTask>,
    ) -> Result<Self, PlanningError> {
        let plan = Self {
            id: Uuid::now_v7(),
            goal_id,
            title: title.into(),
            status: PlanStatus::Ready,
            tasks,
            created_at: Utc::now(),
        };
        plan.validate_dag()?;
        Ok(plan)
    }

    /// Validates that the plan is a valid Directed Acyclic Graph (DAG) with no circular dependencies.
    pub fn validate_dag(&self) -> Result<(), PlanningError> {
        if self.tasks.is_empty() {
            return Err(PlanningError::EmptyPlan);
        }

        let task_ids: HashSet<TaskId> = self.tasks.iter().map(|t| t.id).collect();

        // 1. Verify all prerequisites exist in the plan
        for task in &self.tasks {
            for prereq in &task.prerequisite_task_ids {
                if !task_ids.contains(prereq) {
                    return Err(PlanningError::MissingPrerequisite(task.id, *prereq));
                }
            }
        }

        // 2. Kahn's algorithm for cycle detection
        let mut in_degree: HashMap<TaskId, usize> = HashMap::new();
        let mut adj: HashMap<TaskId, Vec<TaskId>> = HashMap::new();

        for task in &self.tasks {
            in_degree.insert(task.id, task.prerequisite_task_ids.len());
            for prereq in &task.prerequisite_task_ids {
                adj.entry(*prereq).or_default().push(task.id);
            }
        }

        let mut queue: Vec<TaskId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut visited_count = 0;
        while let Some(curr) = queue.pop() {
            visited_count += 1;
            if let Some(dependents) = adj.get(&curr) {
                for dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(*dep);
                        }
                    }
                }
            }
        }

        if visited_count != self.tasks.len() {
            return Err(PlanningError::CyclicDependency);
        }

        Ok(())
    }

    /// Returns tasks ready for execution whose prerequisites are completely satisfied.
    pub fn get_ready_tasks(&self, completed_tasks: &HashSet<TaskId>) -> Vec<&PlannedTask> {
        self.tasks
            .iter()
            .filter(|t| !completed_tasks.contains(&t.id))
            .filter(|t| {
                t.prerequisite_task_ids
                    .iter()
                    .all(|prereq| completed_tasks.contains(prereq))
            })
            .collect()
    }
}
