use cos_goals::{ComparisonOp, Goal, GoalForest, GoalPriority, GoalStatus, SuccessCriterion};

#[test]
fn test_goal_hierarchy_and_progress_propagation() {
    let mut forest = GoalForest::new();

    // 1. Root goal
    let root = Goal::new(
        "Expand Q4 Enterprise Operations",
        "Autonomous pipeline execution across company operations",
    )
    .with_priority(GoalPriority::Critical);
    let root_id = forest.add_goal(root).expect("Add root goal");

    // 2. Sub-goal 1: Lead Qualification
    let sub1 = Goal::new("Automated Qualification", "Process 50 inbound inquiries")
        .with_parent(root_id)
        .with_criterion(SuccessCriterion::new(
            "inquiries_processed",
            50.0,
            ComparisonOp::GreaterThanOrEqual,
            Some("count"),
        ));
    let sub1_id = forest.add_goal(sub1).expect("Add sub1");

    // 3. Sub-goal 2: SLA Response Time
    let sub2 = Goal::new("Response Time SLA", "Maintain < 15 min response time")
        .with_parent(root_id)
        .with_criterion(SuccessCriterion::new(
            "avg_response_minutes",
            15.0,
            ComparisonOp::LessThanOrEqual,
            Some("minutes"),
        ));
    let sub2_id = forest.add_goal(sub2).expect("Add sub2");

    forest.set_status(&sub1_id, GoalStatus::Active).unwrap();
    forest.set_status(&sub2_id, GoalStatus::Active).unwrap();
    forest.set_status(&root_id, GoalStatus::Active).unwrap();

    // Update criteria in sub1: 25 / 50 -> 50%
    forest
        .update_criterion(&sub1_id, "inquiries_processed", 25.0)
        .unwrap();
    let sub1_ref = forest.get_goal(&sub1_id).unwrap();
    assert_eq!(sub1_ref.progress_percent, 50.0);

    // Update criteria in sub2: response time is 10 mins (<= 15 target) -> 100%
    forest
        .update_criterion(&sub2_id, "avg_response_minutes", 10.0)
        .unwrap();
    let sub2_ref = forest.get_goal(&sub2_id).unwrap();
    assert_eq!(sub2_ref.progress_percent, 100.0);

    // Auto bubble-up check: Root progress was automatically recomputed!
    let root_ref = forest.get_goal(&root_id).unwrap();
    assert_eq!(root_ref.progress_percent, 75.0);

    // Complete sub1 to 50
    forest
        .update_criterion(&sub1_id, "inquiries_processed", 50.0)
        .unwrap();
    let root_ref_final = forest.get_goal(&root_id).unwrap();
    assert_eq!(root_ref_final.progress_percent, 100.0);
}

#[test]
fn test_custom_subgoal_weighting() {
    let mut forest = GoalForest::new();

    // Goal where direct criteria is 80% important and sub-goals are 20%
    let parent = Goal::new("High Priority Campaign", "Critical marketing launch")
        .with_sub_goal_weight(0.2)
        .with_criterion(SuccessCriterion::new(
            "budget_approved",
            1.0,
            ComparisonOp::GreaterThanOrEqual,
            None,
        ));
    let parent_id = forest.add_goal(parent).unwrap();

    let child = Goal::new("Design Creatives", "Sub-deliverable")
        .with_parent(parent_id)
        .with_criterion(SuccessCriterion::new(
            "assets_count",
            10.0,
            ComparisonOp::GreaterThanOrEqual,
            None,
        ));
    let child_id = forest.add_goal(child).unwrap();

    forest.set_status(&parent_id, GoalStatus::Active).unwrap();
    forest.set_status(&child_id, GoalStatus::Active).unwrap();

    // 1. Direct criteria is 100% met, child is 0% met
    forest
        .update_criterion(&parent_id, "budget_approved", 1.0)
        .unwrap();
    let progress = forest.recompute_progress(&parent_id).unwrap();
    // 100 * 0.8 + 0 * 0.2 = 80.0
    assert_eq!(progress, 80.0);
}

#[test]
fn test_human_signoff_requirement() {
    let mut forest = GoalForest::new();

    // Goal requiring explicit human signoff
    let goal = Goal::new("Financial Reconciliation", "Audit closing balance")
        .with_human_signoff(true)
        .with_criterion(
            SuccessCriterion::new("accounts_reconciled", 1.0, ComparisonOp::Equal, None)
                .with_tolerance(0.01),
        );
    let goal_id = forest.add_goal(goal).unwrap();
    forest.set_status(&goal_id, GoalStatus::Active).unwrap();

    // Hit 100% progress
    forest
        .update_criterion(&goal_id, "accounts_reconciled", 1.005)
        .unwrap();
    let g = forest.get_goal(&goal_id).unwrap();
    assert_eq!(g.progress_percent, 100.0);
    // Must REMAIN Active (not auto-achieved) because require_human_signoff is true!
    assert_eq!(g.status, GoalStatus::Active);
}

#[test]
fn test_cycle_prevention() {
    let mut forest = GoalForest::new();
    let g1 = forest.add_goal(Goal::new("G1", "Desc")).unwrap();
    let g2 = forest
        .add_goal(Goal::new("G2", "Desc").with_parent(g1))
        .unwrap();

    // Setting G1's parent to G2 would cause cycle G1 -> G2 -> G1
    assert!(!forest.validate_no_cycles(&g1, &g2));
}

#[test]
fn test_anomaly_detection_and_governed_self_goal() {
    let detector = cos_goals::AnomalyDetector::new();
    let governor = cos_goals::SelfGoalGovernor::default();

    // 1. Goal with zero progress triggers anomaly
    let mut stalled_goal = Goal::new("Customer Onboarding", "Streamline client onboarding")
        .with_criterion(SuccessCriterion::new(
            "clients_onboarded",
            10.0,
            ComparisonOp::GreaterThanOrEqual,
            None,
        ));
    stalled_goal.status = GoalStatus::Active;

    let anomaly = detector
        .evaluate_goal(&stalled_goal)
        .expect("Expected anomaly");
    assert_eq!(anomaly.kind, cos_goals::AnomalyKind::MetricDrift);

    // 2. Generate candidate self-goal
    let candidate = governor.generate_remediation_goal(&anomaly, stalled_goal.id);
    assert_eq!(candidate.status, GoalStatus::Proposed);
    assert_eq!(candidate.parent_goal_id, Some(stalled_goal.id));

    // 3. Validation: valid candidate goal succeeds
    assert!(governor.validate_candidate_goal(&candidate, 1).is_ok());

    // 4. Invariant: Orphan goal (no parent) is strictly rejected
    let mut orphan = candidate.clone();
    orphan.parent_goal_id = None;
    assert_eq!(
        governor.validate_candidate_goal(&orphan, 1),
        Err(cos_goals::SelfGoalError::OrphanGoalNotAllowed)
    );

    // 5. Invariant: Max depth exceeded is rejected
    assert_eq!(
        governor.validate_candidate_goal(&candidate, 10),
        Err(cos_goals::SelfGoalError::MaxDepthExceeded(4))
    );
}
