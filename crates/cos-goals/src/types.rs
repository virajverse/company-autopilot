use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Strongly typed Goal identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GoalId(pub Uuid);

impl GoalId {
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Display for GoalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Priority tier of an operational or strategic goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalPriority {
    Low,
    #[default]
    Medium,
    High,
    Critical,
}

/// Lifecycle status of a goal in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    #[default]
    Draft,
    Proposed,
    Active,
    Blocked,
    Achieved,
    Failed,
    Abandoned,
}

/// Comparison operator used to evaluate quantitative criteria.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equal,
}

/// Metric-based success criterion for evaluating goal completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuccessCriterion {
    pub name: String,
    pub target_value: f64,
    pub current_value: f64,
    pub comparison: ComparisonOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Configurable numerical tolerance for equality comparisons (defaults to 0.001).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
}

impl SuccessCriterion {
    pub fn new(
        name: impl Into<String>,
        target_value: f64,
        comparison: ComparisonOp,
        unit: Option<&str>,
    ) -> Self {
        Self {
            name: name.into(),
            target_value,
            current_value: 0.0,
            comparison,
            unit: unit.map(|s| s.to_string()),
            tolerance: None,
        }
    }

    pub fn with_tolerance(mut self, tol: f64) -> Self {
        self.tolerance = Some(tol);
        self
    }

    /// Evaluates whether this specific criterion is currently met using configurable tolerance.
    pub fn is_met(&self) -> bool {
        let tol = self.tolerance.unwrap_or(0.001);
        match self.comparison {
            ComparisonOp::GreaterThan => self.current_value > self.target_value,
            ComparisonOp::GreaterThanOrEqual => self.current_value >= (self.target_value - tol),
            ComparisonOp::LessThan => self.current_value < self.target_value,
            ComparisonOp::LessThanOrEqual => self.current_value <= (self.target_value + tol),
            ComparisonOp::Equal => (self.current_value - self.target_value).abs() <= tol,
        }
    }

    /// Calculates progress ratio (0.0 to 1.0) towards target.
    pub fn progress_ratio(&self) -> f32 {
        if self.target_value == 0.0 {
            return if self.is_met() { 1.0 } else { 0.0 };
        }
        match self.comparison {
            ComparisonOp::GreaterThan | ComparisonOp::GreaterThanOrEqual => {
                let ratio = (self.current_value / self.target_value) as f32;
                ratio.clamp(0.0, 1.0)
            }
            ComparisonOp::LessThan | ComparisonOp::LessThanOrEqual => {
                if self.current_value <= self.target_value {
                    1.0
                } else {
                    let diff = self.current_value - self.target_value;
                    let denom = self.target_value.abs().max(1.0);
                    (1.0 - (diff / denom) as f32).clamp(0.0, 1.0)
                }
            }
            ComparisonOp::Equal => {
                let tol = self.tolerance.unwrap_or(0.001);
                if (self.current_value - self.target_value).abs() <= tol {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// A node in the hierarchical Goal Forest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: GoalId,
    /// Tenant or organization owning this strategic goal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<cos_domain::TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_goal_id: Option<GoalId>,
    pub title: String,
    pub description: String,
    pub priority: GoalPriority,
    pub status: GoalStatus,
    #[serde(default)]
    pub criteria: Vec<SuccessCriterion>,
    pub progress_percent: f32,
    /// Configurable weight assigned to sub-goals vs direct criteria (0.0 to 1.0, default 0.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_goal_weight: Option<f32>,
    /// When true, reaching 100% does NOT auto-mark Achieved without explicit human verification.
    #[serde(default)]
    pub require_human_signoff: bool,
    /// Semantic classification tags (e.g. ["sales", "onboarding", "sla"]).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Explicit hint pointing to an operational SOP key in Memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sop_key_hint: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_date: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

impl Goal {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: GoalId::new_v7(),
            tenant_id: None,
            parent_goal_id: None,
            title: title.into(),
            description: description.into(),
            priority: GoalPriority::Medium,
            status: GoalStatus::Draft,
            criteria: Vec::new(),
            progress_percent: 0.0,
            sub_goal_weight: None,
            require_human_signoff: false,
            tags: Vec::new(),
            sop_key_hint: None,
            created_at: Utc::now(),
            target_date: None,
            completed_at: None,
        }
    }

    pub fn with_tenant(mut self, tenant_id: cos_domain::TenantId) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    pub fn with_parent(mut self, parent_id: GoalId) -> Self {
        self.parent_goal_id = Some(parent_id);
        self
    }

    pub fn with_priority(mut self, priority: GoalPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_criterion(mut self, criterion: SuccessCriterion) -> Self {
        self.criteria.push(criterion);
        self
    }

    pub fn with_sub_goal_weight(mut self, weight: f32) -> Self {
        self.sub_goal_weight = Some(weight.clamp(0.0, 1.0));
        self
    }

    pub fn with_human_signoff(mut self, require: bool) -> Self {
        self.require_human_signoff = require;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_sop_key_hint(mut self, sop_key: impl Into<String>) -> Self {
        self.sop_key_hint = Some(sop_key.into());
        self
    }
}
