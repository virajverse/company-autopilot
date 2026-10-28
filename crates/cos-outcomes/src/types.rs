use chrono::{DateTime, Utc};
use cos_domain::TaskId;
use cos_goals::GoalId;
use cos_identity::ActorIdentity;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Evaluated outcome status of a completed operational task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Success,
    PartialSuccess,
    Failure,
    Inconclusive,
}

/// A measured change in a specific business or system metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricDelta {
    pub metric_name: String,
    pub previous_value: f64,
    pub new_value: f64,
    pub delta: f64,
}

impl MetricDelta {
    pub fn new(metric_name: impl Into<String>, previous_value: f64, new_value: f64) -> Self {
        Self {
            metric_name: metric_name.into(),
            previous_value,
            new_value,
            delta: new_value - previous_value,
        }
    }
}

/// An immutable record of an operational execution outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutcomeRecord {
    pub id: Uuid,
    pub task_id: TaskId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<GoalId>,
    pub status: OutcomeStatus,
    pub summary: String,
    #[serde(default)]
    pub metrics: Vec<MetricDelta>,
    pub confidence_score: f32,
    pub evaluator: ActorIdentity,
    pub created_at: DateTime<Utc>,
}

impl OutcomeRecord {
    pub fn new(
        task_id: TaskId,
        status: OutcomeStatus,
        summary: impl Into<String>,
        confidence_score: f32,
        evaluator: ActorIdentity,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            task_id,
            goal_id: None,
            status,
            summary: summary.into(),
            metrics: Vec::new(),
            confidence_score: confidence_score.clamp(0.0, 1.0),
            evaluator,
            created_at: Utc::now(),
        }
    }

    pub fn with_goal(mut self, goal_id: GoalId) -> Self {
        self.goal_id = Some(goal_id);
        self
    }

    pub fn with_metric(mut self, metric: MetricDelta) -> Self {
        self.metrics.push(metric);
        self
    }
}
