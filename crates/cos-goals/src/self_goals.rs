use crate::types::{ComparisonOp, Goal, GoalId, GoalPriority, GoalStatus, SuccessCriterion};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SelfGoalError {
    #[error("Candidate goal rejected: must be linked to a valid parent goal")]
    OrphanGoalNotAllowed,

    #[error("Candidate goal rejected: exceeds maximum allowed sub-goal depth ({0})")]
    MaxDepthExceeded(usize),

    #[error("Candidate goal rejected: requires explicit human sign-off for high-impact scope")]
    HumanSignoffRequired,

    #[error("Candidate goal rejected: {0}")]
    ValidationFailed(String),
}

/// Severity classification of detected operational anomalies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Type of operational anomaly or knowledge gap detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyKind {
    /// Repeated failure in task execution or metric achievement.
    PerformanceDegradation,
    /// Missing procedure or SOP guideline in Memory.
    KnowledgeGap,
    /// Client SLA or response time breach.
    SlaBreach,
    /// Metric variance exceeding allowed tolerance.
    MetricDrift,
}

/// Structured record of an operational anomaly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAnomaly {
    pub id: Uuid,
    pub kind: AnomalyKind,
    pub severity: AnomalySeverity,
    pub description: String,
    pub related_goal_id: Option<GoalId>,
    pub detected_at: chrono::DateTime<Utc>,
}

/// Anomaly detector that analyzes metrics and performance indicators.
#[derive(Debug, Clone, Default)]
pub struct AnomalyDetector;

impl AnomalyDetector {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates a goal's criteria and progress to detect potential anomalies.
    pub fn evaluate_goal(&self, goal: &Goal) -> Option<DetectedAnomaly> {
        // 1. Detect failed or severely lagging goals
        if goal.status == GoalStatus::Failed {
            return Some(DetectedAnomaly {
                id: Uuid::now_v7(),
                kind: AnomalyKind::PerformanceDegradation,
                severity: AnomalySeverity::High,
                description: format!("Goal '{}' has failed and requires remediation", goal.title),
                related_goal_id: Some(goal.id),
                detected_at: Utc::now(),
            });
        }

        // 2. Detect unmet criteria with zero progress
        let has_unmet = goal
            .criteria
            .iter()
            .any(|c| !c.is_met() && c.current_value == 0.0);
        if has_unmet && goal.progress_percent == 0.0 && goal.status == GoalStatus::Active {
            return Some(DetectedAnomaly {
                id: Uuid::now_v7(),
                kind: AnomalyKind::MetricDrift,
                severity: AnomalySeverity::Medium,
                description: format!(
                    "Goal '{}' is active but has zero progress towards criteria",
                    goal.title
                ),
                related_goal_id: Some(goal.id),
                detected_at: Utc::now(),
            });
        }

        None
    }
}

/// Generator and validator for governed candidate self-goals (Phase 9).
#[derive(Debug, Clone)]
pub struct SelfGoalGovernor {
    max_subgoal_depth: usize,
}

impl Default for SelfGoalGovernor {
    fn default() -> Self {
        Self {
            max_subgoal_depth: 4,
        }
    }
}

impl SelfGoalGovernor {
    pub fn new(max_subgoal_depth: usize) -> Self {
        Self { max_subgoal_depth }
    }

    /// Generates a governed candidate self-goal to remediate a detected anomaly.
    pub fn generate_remediation_goal(
        &self,
        anomaly: &DetectedAnomaly,
        parent_goal_id: GoalId,
    ) -> Goal {
        let priority = match anomaly.severity {
            AnomalySeverity::Critical => GoalPriority::Critical,
            AnomalySeverity::High => GoalPriority::High,
            AnomalySeverity::Medium => GoalPriority::Medium,
            AnomalySeverity::Low => GoalPriority::Low,
        };

        let require_human_signoff = matches!(
            anomaly.severity,
            AnomalySeverity::Critical | AnomalySeverity::High
        );

        let mut goal = Goal::new(
            format!("Remediate: {}", anomaly.description),
            format!(
                "Self-generated goal to resolve {:?} anomaly: {}",
                anomaly.kind, anomaly.description
            ),
        )
        .with_priority(priority);

        goal.parent_goal_id = Some(parent_goal_id);
        goal.status = GoalStatus::Proposed;
        goal.require_human_signoff = require_human_signoff;

        // Attach target criterion for remediation
        goal.criteria.push(SuccessCriterion {
            name: "anomaly_resolution_rate".to_string(),
            target_value: 1.0,
            current_value: 0.0,
            unit: Some("ratio".to_string()),
            comparison: ComparisonOp::GreaterThanOrEqual,
            tolerance: None,
        });

        goal
    }

    /// Validates a candidate self-goal against company hierarchy and authority invariants.
    pub fn validate_candidate_goal(&self, goal: &Goal, depth: usize) -> Result<(), SelfGoalError> {
        // Law: Self-goal cannot bypass company goal hierarchy (must have parent)
        if goal.parent_goal_id.is_none() {
            return Err(SelfGoalError::OrphanGoalNotAllowed);
        }

        // Bounded depth invariant
        if depth > self.max_subgoal_depth {
            return Err(SelfGoalError::MaxDepthExceeded(self.max_subgoal_depth));
        }

        // High priority self-goals must require human signoff
        if (goal.priority == GoalPriority::Critical || goal.priority == GoalPriority::High)
            && !goal.require_human_signoff
        {
            return Err(SelfGoalError::HumanSignoffRequired);
        }

        Ok(())
    }
}
