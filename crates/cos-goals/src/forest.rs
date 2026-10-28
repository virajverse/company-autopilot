use crate::types::{Goal, GoalId, GoalStatus};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum GoalError {
    #[error("Goal '{0}' not found in goal registry")]
    GoalNotFound(GoalId),

    #[error("Criterion '{0}' not found in goal '{1}'")]
    CriterionNotFound(String, GoalId),

    #[error("Circular parent relationship detected for goal '{0}'")]
    CircularDependency(GoalId),

    #[error("Invalid status transition from {from:?} to {to:?}")]
    InvalidTransition { from: GoalStatus, to: GoalStatus },
}

/// Dynamic hierarchical container managing trees of business and operational goals.
#[derive(Debug, Clone, Default)]
pub struct GoalForest {
    goals: HashMap<GoalId, Goal>,
}

impl GoalForest {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new goal to the forest, verifying hierarchy validity.
    pub fn add_goal(&mut self, goal: Goal) -> Result<GoalId, GoalError> {
        if let Some(parent_id) = goal.parent_goal_id {
            if !self.goals.contains_key(&parent_id) {
                return Err(GoalError::GoalNotFound(parent_id));
            }
        }
        let id = goal.id;
        self.goals.insert(id, goal);
        Ok(id)
    }

    /// Returns an iterator over references to all goals in the forest.
    pub fn iter(&self) -> impl Iterator<Item = &Goal> {
        self.goals.values()
    }

    /// Returns a list of all goals in the forest.
    pub fn list_all(&self) -> Vec<Goal> {
        self.goals.values().cloned().collect()
    }

    /// Retrieves a reference to a goal.
    pub fn get_goal(&self, id: &GoalId) -> Option<&Goal> {
        self.goals.get(id)
    }

    /// Retrieves a mutable reference to a goal.
    pub fn get_goal_mut(&mut self, id: &GoalId) -> Option<&mut Goal> {
        self.goals.get_mut(id)
    }

    /// Sets the status of a goal, handling completion timestamps.
    pub fn set_status(&mut self, id: &GoalId, new_status: GoalStatus) -> Result<(), GoalError> {
        let goal = self.goals.get_mut(id).ok_or(GoalError::GoalNotFound(*id))?;
        goal.status = new_status;
        if new_status == GoalStatus::Achieved {
            goal.completed_at = Some(Utc::now());
            goal.progress_percent = 100.0;
        }
        Ok(())
    }

    /// Updates the current metric value of a criterion on a goal.
    pub fn update_criterion(
        &mut self,
        goal_id: &GoalId,
        criterion_name: &str,
        new_value: f64,
    ) -> Result<bool, GoalError> {
        let goal = self
            .goals
            .get_mut(goal_id)
            .ok_or(GoalError::GoalNotFound(*goal_id))?;

        let crit = goal
            .criteria
            .iter_mut()
            .find(|c| c.name == criterion_name)
            .ok_or_else(|| GoalError::CriterionNotFound(criterion_name.to_string(), *goal_id))?;

        crit.current_value = new_value;
        let is_met = crit.is_met();

        // Recompute progress for this goal
        self.recompute_progress(goal_id)?;

        // BUBBLE UP: Automatically propagate progress to all ancestor goals in the hierarchy
        let mut current_parent = self.goals.get(goal_id).and_then(|g| g.parent_goal_id);
        while let Some(parent_id) = current_parent {
            self.recompute_progress(&parent_id)?;
            current_parent = self.goals.get(&parent_id).and_then(|g| g.parent_goal_id);
        }

        Ok(is_met)
    }

    /// Recomputes progress percentage based on criteria and child goals.
    pub fn recompute_progress(&mut self, goal_id: &GoalId) -> Result<f32, GoalError> {
        if !self.goals.contains_key(goal_id) {
            return Err(GoalError::GoalNotFound(*goal_id));
        }

        // Child progress
        let children_ids: Vec<GoalId> = self
            .goals
            .values()
            .filter(|g| g.parent_goal_id == Some(*goal_id))
            .map(|g| g.id)
            .collect();

        let goal = self.goals.get_mut(goal_id).unwrap();

        let sub_weight = goal.sub_goal_weight.unwrap_or(0.5).clamp(0.0, 1.0);
        let crit_weight = 1.0 - sub_weight;

        let total_progress = if children_ids.is_empty() {
            if goal.criteria.is_empty() {
                0.0
            } else {
                let sum: f32 = goal.criteria.iter().map(|c| c.progress_ratio()).sum();
                (sum / (goal.criteria.len() as f32)) * 100.0
            }
        } else if goal.criteria.is_empty() {
            let children_progress_sum: f32 = children_ids
                .iter()
                .filter_map(|cid| self.goals.get(cid))
                .map(|cg| cg.progress_percent)
                .sum();
            children_progress_sum / (children_ids.len() as f32)
        } else {
            let sum: f32 = goal.criteria.iter().map(|c| c.progress_ratio()).sum();
            let criteria_ratio = sum / (goal.criteria.len() as f32);

            let children_progress_sum: f32 = children_ids
                .iter()
                .filter_map(|cid| self.goals.get(cid))
                .map(|cg| cg.progress_percent)
                .sum();
            let children_avg = children_progress_sum / (children_ids.len() as f32);

            // Dynamic configurable weighting between criteria and subgoals
            (criteria_ratio * crit_weight * 100.0) + (children_avg * sub_weight)
        };

        let clamped = total_progress.clamp(0.0, 100.0);
        let goal = self.goals.get_mut(goal_id).unwrap();
        goal.progress_percent = clamped;

        // Auto-check achievement only if human signoff is not explicitly required
        if clamped >= 100.0 && goal.status == GoalStatus::Active && !goal.require_human_signoff {
            goal.status = GoalStatus::Achieved;
            goal.completed_at = Some(Utc::now());
        }

        Ok(clamped)
    }

    /// Returns direct children of a given goal.
    pub fn get_sub_goals(&self, parent_id: &GoalId) -> Vec<&Goal> {
        self.goals
            .values()
            .filter(|g| g.parent_goal_id == Some(*parent_id))
            .collect()
    }

    /// Returns all top-level (root) goals.
    pub fn get_root_goals(&self) -> Vec<&Goal> {
        self.goals
            .values()
            .filter(|g| g.parent_goal_id.is_none())
            .collect()
    }

    /// Returns all goals currently in `Active` status.
    pub fn get_active_goals(&self) -> Vec<&Goal> {
        self.goals
            .values()
            .filter(|g| g.status == GoalStatus::Active)
            .collect()
    }

    /// Checks if a circular relationship would be created.
    pub fn validate_no_cycles(&self, start_id: &GoalId, target_parent: &GoalId) -> bool {
        if start_id == target_parent {
            return false;
        }
        let mut visited = HashSet::new();
        visited.insert(*start_id);
        let mut curr = Some(*target_parent);

        while let Some(parent) = curr {
            if visited.contains(&parent) {
                return false;
            }
            visited.insert(parent);
            curr = self.goals.get(&parent).and_then(|g| g.parent_goal_id);
        }
        true
    }
}
