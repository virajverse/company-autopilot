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

    info!("Cold-start detected: Initializing Memory with 8 canonical specialist roles, escalation policies, and SOPs...");
    let actor = ActorIdentity::system("cold_start_bootstrapper");

    // 1. Sales and Lead Role SOP
    let sales_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:sales_and_lead",
        serde_json::json!({
            "role": "sales_and_lead",
            "mission": "Qualify inbound inquiries, calculate deal sizes, and prepare initial commercial proposals",
            "auto_dispatch_allowed": true,
            "max_response_time_minutes": 15,
            "default_tool": "notification_sender",
            "default_capability": "send_notification",
            "escalation_policy_key": "policy:commercial_escalation",
            "reporting_to": "company_executive",
            "qualification_rubric": {
                "budget_threshold_usd": 5000,
                "preferred_timeline_weeks": 8,
                "disallowed_project_types": ["gambling", "crypto_schemes", "unlicensed_trading"]
            },
            "allowed_tools": ["notification_sender", "http_webhook", "web_research"]
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
            "default_tool": "file_deliverable",
            "default_capability": "filesystem_write",
            "escalation_policy_key": "policy:commercial_escalation",
            "reporting_to": "company_executive",
            "estimation_rubric": {
                "contingency_buffer_percentage": 20,
                "architecture_review_required": true
            },
            "deliverable_format": "markdown_spec",
            "allowed_tools": ["file_deliverable", "notification_sender", "web_research"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&discovery_sop)?;
    let _ = mem_store.store(discovery_sop);

    // 3. Technical Architect & Coder Role SOP
    let coder_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:tech_architect_and_coder",
        serde_json::json!({
            "role": "tech_architect_and_coder",
            "mission": "Design software architectures, produce clean production code, and implement technical deliverables",
            "auto_dispatch_allowed": true,
            "default_tool": "file_deliverable",
            "default_capability": "filesystem_write",
            "escalation_policy_key": "policy:technical_escalation",
            "reporting_to": "company_executive",
            "code_standards": {
                "pure_cpu_runtime": true,
                "zero_hardcoded_credentials": true,
                "compile_clean_without_warnings": true
            },
            "allowed_tools": ["file_deliverable", "web_research"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&coder_sop)?;
    let _ = mem_store.store(coder_sop);

    // 4. QA and Deliverable Auditor Role SOP
    let qa_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:qa_and_auditor",
        serde_json::json!({
            "role": "qa_and_auditor",
            "mission": "Verify code quality, run deliverable audits, validate security boundaries, and ensure compliance",
            "auto_dispatch_allowed": true,
            "default_tool": "file_deliverable",
            "default_capability": "filesystem_write",
            "escalation_policy_key": "policy:technical_escalation",
            "reporting_to": "company_executive",
            "audit_criteria": ["zero_mock_in_prod", "no_redos_regex", "client_isolation_preserved"],
            "allowed_tools": ["file_deliverable", "notification_sender", "web_research"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&qa_sop)?;
    let _ = mem_store.store(qa_sop);

    // 5. Project Operations Role SOP
    let operations_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:project_operations",
        serde_json::json!({
            "role": "project_operations",
            "mission": "Coordinate task DAG execution, milestone validation, sprint tracking, and deliverable handoffs",
            "auto_dispatch_allowed": true,
            "default_tool": "file_deliverable",
            "default_capability": "filesystem_write",
            "escalation_policy_key": "policy:operational_escalation",
            "reporting_to": "company_executive",
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

    // 6. Client Success and Communication Role SOP
    let client_success_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:client_success_communication",
        serde_json::json!({
            "role": "client_success_communication",
            "mission": "Maintain client relationship health, onboarding satisfaction, milestone updates, and feedback loops",
            "auto_dispatch_allowed": true,
            "default_tool": "notification_sender",
            "default_capability": "send_notification",
            "escalation_policy_key": "policy:commercial_escalation",
            "reporting_to": "company_executive",
            "cadence_days": 7,
            "sentiment_threshold": "neutral_or_better",
            "allowed_tools": ["notification_sender", "http_webhook"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&client_success_sop)?;
    let _ = mem_store.store(client_success_sop);

    // 7. Legal and Regulatory Compliance Role SOP
    let legal_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:legal_and_compliance",
        serde_json::json!({
            "role": "legal_and_compliance",
            "mission": "Review service agreements, check open-source licenses, verify intellectual property, and enforce regulations",
            "auto_dispatch_allowed": false, // Legal agreements require strict Founder signoff
            "default_tool": "file_deliverable",
            "default_capability": "filesystem_write",
            "escalation_policy_key": "policy:legal_escalation",
            "reporting_to": "founder",
            "forbidden_clauses": ["unlimited_indemnity", "exclusive_lock_in", "waiver_of_ip"],
            "allowed_tools": ["file_deliverable", "web_research", "notification_sender"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&legal_sop)?;
    let _ = mem_store.store(legal_sop);

    // 8. Research and Intelligence Role SOP
    let research_sop = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Guideline,
        "sop:research_and_intel",
        serde_json::json!({
            "role": "research_and_intel",
            "mission": "Investigate market trends, synthesize technical documentation, benchmark competitors, and gather verified facts",
            "auto_dispatch_allowed": true,
            "default_tool": "web_research",
            "default_capability": "web_research",
            "escalation_policy_key": "policy:technical_escalation",
            "reporting_to": "company_executive",
            "allowed_tools": ["web_research", "file_deliverable"]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&research_sop)?;
    let _ = mem_store.store(research_sop);

    // 9. Commercial Escalation Policy
    let commercial_policy = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Procedure,
        "policy:commercial_escalation",
        serde_json::json!({
            "policy_key": "policy:commercial_escalation",
            "rules": [
                {
                    "trigger": "discount_requested",
                    "conditions": { "max_discount_percent": 10.0 },
                    "resolution": "executive_auto_approve",
                    "target_role": "company_executive"
                },
                {
                    "trigger": "discount_requested",
                    "conditions": { "min_discount_percent": 10.01 },
                    "resolution": "founder_approval_mandated",
                    "target_identity": "founder"
                },
                {
                    "trigger": "unknown_industry_vertical",
                    "resolution": "trigger_web_research_then_founder"
                }
            ]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&commercial_policy)?;
    let _ = mem_store.store(commercial_policy);

    // 10. Technical Escalation Policy
    let technical_policy = MemoryRecord::new(
        MemoryNamespace::Company,
        MemoryKind::Procedure,
        "policy:technical_escalation",
        serde_json::json!({
            "policy_key": "policy:technical_escalation",
            "rules": [
                {
                    "trigger": "missing_technical_documentation",
                    "resolution": "invoke_tier3_web_research",
                    "target_tool": "web_research"
                },
                {
                    "trigger": "production_security_anomaly",
                    "resolution": "founder_immediate_veto",
                    "target_identity": "founder"
                }
            ]
        }),
        cos_domain::KnowledgeLifecycle::Verified,
        1.0,
        ProvenanceType::Learned,
        actor.clone(),
    );
    store.save_record(&technical_policy)?;
    let _ = mem_store.store(technical_policy);

    // 11. Default Planning Decomposition Template
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
                    "title": "Technical Architecture and Implementation",
                    "role": "tech_architect_and_coder",
                    "target_tool": "file_deliverable",
                    "required_capability": "filesystem_write",
                    "risk_class": "R1_REVERSIBLE_LOW_IMPACT",
                    "dependencies": [0]
                },
                {
                    "title": "Quality and Security Verification",
                    "role": "qa_and_auditor",
                    "target_tool": "file_deliverable",
                    "required_capability": "filesystem_write",
                    "risk_class": "R1_REVERSIBLE_LOW_IMPACT",
                    "dependencies": [1]
                },
                {
                    "title": "Execution and Deliverable Production",
                    "role": "project_operations",
                    "target_tool": "file_deliverable",
                    "required_capability": "filesystem_write",
                    "risk_class": "R1_REVERSIBLE_LOW_IMPACT",
                    "dependencies": [2]
                },
                {
                    "title": "Client Handover and Communication",
                    "role": "client_success_communication",
                    "target_tool": "notification_sender",
                    "required_capability": "send_notification",
                    "risk_class": "R2_EXTERNAL_COMMUNICATION",
                    "dependencies": [3]
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
        "Cold-start initialization complete: Seeded 8 canonical specialist roles, escalation policies, and HTN planning template."
    );
    Ok(true)
}
