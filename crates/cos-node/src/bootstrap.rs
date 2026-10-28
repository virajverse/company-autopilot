use cos_identity::ActorIdentity;
use cos_memory::{MemoryKind, MemoryNamespace, MemoryRecord, ProvenanceType};
use cos_persistence::MemoryRepository;
use tracing::info;

/// Seeds initial company operating procedures (SOPs) and planning templates into Memory on cold start.
///
/// Invariant: Completely brand-agnostic, white-label, and memory-driven.
/// All operational parameters, rubrics, and templates are editable and versioned in Memory.
pub fn seed_company_memory(
    store: &(impl MemoryRepository + ?Sized),
    mem_store: &cos_memory::MemoryStore,
) -> Result<bool, Box<dyn std::error::Error>> {
    let check = store.get_record(&MemoryNamespace::Company, "sop:sales_and_lead")?;
    if check.is_some() {
        // Also ensure in-memory store has the persisted records
        for rec in store.list_records(&MemoryNamespace::Company)? {
            let _ = mem_store.store(rec);
        }
        return Ok(false); // Already seeded
    }

    info!("Cold-start detected: Initializing Memory with 4 canonical MVP roles and SOPs...");
    let actor = ActorIdentity::system("cold_start_bootstrapper");

    // 1. Sales and Lead Role SOP
    let sales_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:sales_and_lead",
        serde_json::json!({
            "role": "sales_and_lead",
            "mission": "Qualify inbound inquiries and prepare structured follow-up proposals",
            "auto_dispatch_allowed": true,
            "max_response_time_minutes": 15,
            "qualification_rubric": {
                "budget_threshold_usd": 5000,
                "preferred_timeline_weeks": 8,
                "disallowed_project_types": ["gambling", "crypto_schemes", "unlicensed_trading"]
            },
            "allowed_tools": ["notification_sender", "http_webhook"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&sales_sop)?;
    let _ = mem_store.store(sales_sop);

    // 2. Discovery and Proposal Role SOP
    let discovery_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:discovery_and_proposal",
        serde_json::json!({
            "role": "discovery_and_proposal",
            "mission": "Translate verified client requirements into technical specifications and work proposals",
            "auto_dispatch_allowed": false, // Proposals require human review before binding commitments
            "estimation_rubric": {
                "contingency_buffer_percentage": 20,
                "architecture_review_required": true
            },
            "deliverable_format": "markdown_spec",
            "allowed_tools": ["file_deliverable", "notification_sender"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&discovery_sop)?;
    let _ = mem_store.store(discovery_sop);

    // 3. Project Operations Role SOP
    let operations_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:project_operations",
        serde_json::json!({
            "role": "project_operations",
            "mission": "Coordinate task DAG execution, milestone validation, and deliverable handoffs",
            "auto_dispatch_allowed": true,
            "task_timeout_hours": 24,
            "max_retries_before_escalation": 2,
            "allowed_tools": ["file_deliverable", "http_webhook"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&operations_sop)?;
    let _ = mem_store.store(operations_sop);

    // 4. Client Success and Communication Role SOP
    let client_success_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:client_success_communication",
        serde_json::json!({
            "role": "client_success_communication",
            "mission": "Maintain client relationship health, onboarding satisfaction, and delivery check-ins",
            "auto_dispatch_allowed": true,
            "cadence_days": 7,
            "sentiment_threshold": "neutral_or_better",
            "allowed_tools": ["notification_sender"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&client_success_sop)?;
    let _ = mem_store.store(client_success_sop);

    // 5. Default Planning Decomposition Template
    let planning_template = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:default_planning_template",
        serde_json::json!({
            "tasks": [
                {
                    "title": "Discovery and Scope Analysis",
                    "role": "discovery_and_proposal",
                    "target_tool": "file_deliverable",
                    "required_capability": "filesystem_write",
                    "risk_class": "R1_REVERSIBLE_LOW_IMPACT",
                    "dependencies": []
                },
                {
                    "title": "Execution and Deliverable Production",
                    "role": "project_operations",
                    "target_tool": "file_deliverable",
                    "required_capability": "filesystem_write",
                    "risk_class": "R1_REVERSIBLE_LOW_IMPACT",
                    "dependencies": [0]
                },
                {
                    "title": "Client Handover and Communication",
                    "role": "client_success_communication",
                    "target_tool": "notification_sender",
                    "required_capability": "send_notification",
                    "risk_class": "R2_EXTERNAL_COMMUNICATION",
                    "dependencies": [1]
                }
            ]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor,
    );
    store.save_record(&planning_template)?;
    let _ = mem_store.store(planning_template);

    info!(
        "Cold-start initialization complete: Seeded 4 canonical roles and HTN planning template."
    );
    Ok(true)
}
