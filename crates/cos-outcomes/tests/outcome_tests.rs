use cos_domain::TaskId;
use cos_goals::GoalId;
use cos_identity::ActorIdentity;
use cos_outcomes::{MetricDelta, OutcomeRecord, OutcomeRegistry, OutcomeStatus};

#[test]
fn test_outcome_registry_tracking() {
    let registry = OutcomeRegistry::new();
    let goal_id = GoalId::new_v7();
    let actor = ActorIdentity::system("outcome-monitor");

    // Record Success
    let t1 = TaskId::new_v7();
    let r1 = OutcomeRecord::new(
        t1,
        OutcomeStatus::Success,
        "Qualified inbound inquiry",
        0.95,
        actor.clone(),
    )
    .with_goal(goal_id)
    .with_metric(MetricDelta::new("response_time_sec", 120.0, 45.0));
    registry.record(r1).unwrap();

    // Record Partial
    let t2 = TaskId::new_v7();
    let r2 = OutcomeRecord::new(
        t2,
        OutcomeStatus::PartialSuccess,
        "Drafted proposal",
        0.70,
        actor.clone(),
    )
    .with_goal(goal_id);
    registry.record(r2).unwrap();

    // Record Failure
    let t3 = TaskId::new_v7();
    let r3 = OutcomeRecord::new(
        t3,
        OutcomeStatus::Failure,
        "API webhook timed out",
        0.99,
        actor,
    )
    .with_goal(goal_id);
    registry.record(r3).unwrap();

    let fetched = registry.get_by_task(&t1).expect("Get by task");
    assert_eq!(fetched.status, OutcomeStatus::Success);
    assert_eq!(fetched.metrics[0].delta, -75.0);

    let goal_outcomes = registry.list_by_goal(&goal_id);
    assert_eq!(goal_outcomes.len(), 3);

    // 1 success out of 3 = 33.33%
    let rate = registry.calculate_success_rate();
    assert!((rate - 33.333).abs() < 0.1);

    let unsuccessful = registry.list_unsuccessful();
    assert_eq!(unsuccessful.len(), 1);
    assert_eq!(unsuccessful[0].task_id, t3);
}
