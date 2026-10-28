use crate::types::{OutcomeRecord, OutcomeStatus};
use cos_domain::TaskId;
use cos_goals::GoalId;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum OutcomeError {
    #[error("Outcome '{0}' already recorded")]
    DuplicateOutcome(Uuid),

    #[error("Outcome not found for task '{0}'")]
    NotFound(TaskId),
}

/// In-memory thread-safe registry tracking outcomes across all autonomous tasks.
#[derive(Debug, Clone, Default)]
pub struct OutcomeRegistry {
    records: Arc<RwLock<HashMap<Uuid, OutcomeRecord>>>,
    task_index: Arc<RwLock<HashMap<TaskId, Uuid>>>,
}

impl OutcomeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores an outcome record.
    pub fn record(&self, record: OutcomeRecord) -> Result<Uuid, OutcomeError> {
        let mut records = self.records.write().unwrap();
        let mut task_index = self.task_index.write().unwrap();

        let id = record.id;
        if records.contains_key(&id) {
            return Err(OutcomeError::DuplicateOutcome(id));
        }

        task_index.insert(record.task_id, id);
        records.insert(id, record);
        Ok(id)
    }

    /// Finds the outcome associated with a task ID.
    pub fn get_by_task(&self, task_id: &TaskId) -> Option<OutcomeRecord> {
        let task_index = self.task_index.read().unwrap();
        let records = self.records.read().unwrap();

        task_index
            .get(task_id)
            .and_then(|id| records.get(id))
            .cloned()
    }

    /// Lists all outcomes related to a specific goal.
    pub fn list_by_goal(&self, goal_id: &GoalId) -> Vec<OutcomeRecord> {
        let records = self.records.read().unwrap();
        records
            .values()
            .filter(|r| r.goal_id == Some(*goal_id))
            .cloned()
            .collect()
    }

    /// Calculates the success rate (0.0 to 100.0) across all recorded outcomes.
    pub fn calculate_success_rate(&self) -> f32 {
        let records = self.records.read().unwrap();
        if records.is_empty() {
            return 100.0;
        }

        let successes = records
            .values()
            .filter(|r| r.status == OutcomeStatus::Success)
            .count();

        (successes as f32 / records.len() as f32) * 100.0
    }

    /// Returns all failed or inconclusive outcomes for continuous learning analysis.
    pub fn list_unsuccessful(&self) -> Vec<OutcomeRecord> {
        let records = self.records.read().unwrap();
        records
            .values()
            .filter(|r| {
                r.status == OutcomeStatus::Failure || r.status == OutcomeStatus::Inconclusive
            })
            .cloned()
            .collect()
    }
}
