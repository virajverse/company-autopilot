use cos_domain::KnowledgeLifecycle;
use cos_domain::RiskClass;
use cos_goals::{Goal, GoalId};
use cos_identity::ActorIdentity;
use cos_memory::{MemoryKind, MemoryNamespace, MemoryRecord, MemoryStore, ProvenanceType};
use cos_planning::{DynamicPlanner, ExecutionPlan, PlanStatus, PlannedTask, PlanningError};
use std::collections::HashSet;

#[test]
fn test_plan_dag_validation_and_readiness() {
    let goal_id = GoalId::new_v7();

    let t1 = PlannedTask::new(
        goal_id,
        "T1",
        "Desc1",
        "role-a",
        RiskClass::R1ReversibleLowImpact,
    );
    let t1_id = t1.id;

    let t2 = PlannedTask::new(
        goal_id,
        "T2",
        "Desc2",
        "role-b",
        RiskClass::R2ExternalCommunication,
    )
    .with_prerequisite(t1_id);
    let t2_id = t2.id;

    let t3 = PlannedTask::new(
        goal_id,
        "T3",
        "Desc3",
        "role-c",
        RiskClass::R3BusinessCommitment,
    )
    .with_prerequisite(t2_id);
    let t3_id = t3.id;

    let mut plan = ExecutionPlan::new(goal_id, "Test Plan", vec![t1, t2, t3]).unwrap();
    let planner = DynamicPlanner::new();

    let mut completed = HashSet::new();

    // Initially, only T1 is ready
    let ready = planner.ready_tasks(&plan, &completed);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, t1_id);

    // Complete T1
    completed.insert(t1_id);
    assert_eq!(
        planner.evaluate_plan_status(&mut plan, &completed),
        PlanStatus::Executing
    );
    let ready = planner.ready_tasks(&plan, &completed);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, t2_id);

    // Complete T2
    completed.insert(t2_id);
    let ready = planner.ready_tasks(&plan, &completed);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, t3_id);

    // Complete T3
    completed.insert(t3_id);
    assert_eq!(
        planner.evaluate_plan_status(&mut plan, &completed),
        PlanStatus::Completed
    );
    let ready = planner.ready_tasks(&plan, &completed);
    assert!(ready.is_empty());
}

#[test]
fn test_plan_cycle_detection_fails() {
    let goal_id = GoalId::new_v7();
    let mut t1 = PlannedTask::new(
        goal_id,
        "T1",
        "Desc1",
        "role-a",
        RiskClass::R1ReversibleLowImpact,
    );
    let mut t2 = PlannedTask::new(
        goal_id,
        "T2",
        "Desc2",
        "role-b",
        RiskClass::R1ReversibleLowImpact,
    );

    t1.prerequisite_task_ids.push(t2.id);
    t2.prerequisite_task_ids.push(t1.id);

    let result = ExecutionPlan::new(goal_id, "Cyclic", vec![t1, t2]);
    assert_eq!(result.unwrap_err(), PlanningError::CyclicDependency);
}

#[test]
fn test_memory_driven_sop_planning() {
    let memory = MemoryStore::new();
    let author = ActorIdentity::system("planner_test");

    // Insert dynamic operational SOP into company memory
    let sop_record = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Procedure,
        "client_onboarding",
        serde_json::json!({
            "steps": [
                { "title": "Verify Credentials", "role": "compliance-agent", "risk": "R1" },
                { "title": "Provision Resources", "role": "ops-agent", "risk": "R2" },
                { "title": "Send Welcome Notification", "role": "communications-agent", "risk": "R2" }
            ]
        }),
        KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Observed,
        author,
    );
    memory.store(sop_record).unwrap();

    let goal = Goal::new("Client Onboarding Flow", "Execute standard onboarding SOP");
    let planner = DynamicPlanner::new();

    let plan = planner.plan_from_goal(&goal, &memory).unwrap();
    assert_eq!(plan.tasks.len(), 3);
    assert_eq!(plan.tasks[0].title, "Verify Credentials");
    assert_eq!(plan.tasks[1].title, "Provision Resources");
    assert_eq!(plan.tasks[2].title, "Send Welcome Notification");
}

#[test]
fn test_tag_based_dynamic_sop_matching() {
    let memory = MemoryStore::new();
    let author = ActorIdentity::system("planner_test");

    // SOP title doesn't match goal title, but tags match!
    let sop_record = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Procedure,
        "pipeline_acceleration_sop",
        serde_json::json!({
            "tags": ["b2b_sales", "growth"],
            "steps": [
                { "title": "Score Pipeline", "role": "sales-worker", "risk": "R1" },
                { "title": "Dispatch Outreach", "role": "sales-worker", "risk": "R2" }
            ]
        }),
        KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Observed,
        author,
    );
    memory.store(sop_record).unwrap();

    let goal = Goal::new("Acquire 10 Strategic Accounts", "Q4 expansion").with_tag("b2b_sales");

    let planner = DynamicPlanner::new();
    let plan = planner.plan_from_goal(&goal, &memory).unwrap();

    assert_eq!(plan.tasks.len(), 2);
    assert_eq!(plan.tasks[0].title, "Score Pipeline");
    assert_eq!(plan.tasks[1].title, "Dispatch Outreach");
}

#[test]
fn test_memory_driven_default_template_fallback() {
    let memory = MemoryStore::new();
    let author = ActorIdentity::system("planner_test");

    // Store custom default planning template in memory
    let template_record = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Procedure,
        "default_planning_template",
        serde_json::json!({
            "steps": [
                { "stage": "Analysis & Scoping", "role": "general-worker", "risk": "R1" },
                { "stage": "Verification", "role": "qa-worker", "risk": "R1" }
            ]
        }),
        KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Observed,
        author,
    );
    memory.store(template_record).unwrap();

    // Goal with zero matching SOPs
    let goal = Goal::new("Novel Research Topic", "Unprecedented operations");
    let planner = DynamicPlanner::new();
    let plan = planner.plan_from_goal(&goal, &memory).unwrap();

    assert_eq!(plan.tasks.len(), 2);
    assert_eq!(
        plan.tasks[0].title,
        "Novel Research Topic: Analysis & Scoping"
    );
    assert_eq!(plan.tasks[1].title, "Novel Research Topic: Verification");
}
