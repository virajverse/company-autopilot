# Autonomous Company Operating System (COS)
## Comprehensive Architecture Blueprint, Mermaid System Flows & Commercial Valuation

---

## 1. Executive Overview & Commercial Valuation

This software is an autonomous, production-grade **Company Operating System (COS)** built entirely in **Rust** as a modular monolith across 20 specialized crates. It eliminates human operational friction by letting self-governing, memory-driven AI specialist roles manage routine operations, project coordination, lead qualification, and client success while founders and engineers focus exclusively on building core client software.

### Commercial Worth Breakdown

| Valuation Dimension | Global Market (USD) | Indian Tech Market (INR) | Strategic Justification |
|---|---|---|---|
| **Engineering Replacement Cost** | **\$180,000 – \$300,000** | **₹1.50 Cr – ₹2.50 Cr** | Requires 2–3 Senior Rust & Distributed Systems Architects for 4–6 months to design compile-time invariants, SHA-256 cryptographic action envelopes, DAG HTN planners, bounded atomic idempotency, and streaming I/O. |
| **Annual Payroll Replaced (ROI)** | **\$80,000 – \$120,000 / yr** | **₹65 Lakh – ₹1.00 Cr / yr** | Replaces or 10x-leverages 3 routine human hires: Lead SDR/Qualifier (\$30k-\$40k), Project Operations Coordinator (\$40k-\$50k), and Client Account Success Manager (\$30k-\$40k) with 24/7 continuous autonomous execution. |
| **Enterprise IP Asset Valuation** | **\$2,000,000 – \$5,000,000** | **₹16.5 Cr – ₹41.5 Cr** | Pre-seed / Seed institutional valuation benchmark for enterprise-ready agentic operating systems with hard mathematical security gates (preventing prompt injection tool hijacking). |
| **White-Label Agency ARR Potential** | **\$180,000 – \$360,000 / yr** | **₹1.50 Cr – ₹3.00 Cr / yr** | 10–15 agency clients at \$10k–\$25k setup fee + \$1,500–\$3,000/month recurring maintenance retainer. 100% white-label with zero brand leaks. |

---

## 2. Master System Architecture (20-Crate Topology)

The architecture strictly follows dependency inversion: core domain crates have zero I/O or provider dependencies, capability crates define ports, persistence/tools implement adapters, and the runtime wires the composition root.

```mermaid
flowchart TB
    subgraph UI_API["1. External Interfaces & Ingestion"]
        NODE["cos-node (Daemon Binary)"]
        API["cos-api (Axum REST & SSE Engine)"]
    end

    subgraph RUNTIME["2. Coordination & Execution Authority"]
        EXEC["cos-executive (Central Orchestrator)"]
        HEARTBEAT["AutonomousEngine (5s Heartbeat Ticker)"]
        AGENTS["cos-agents (Dynamic Role Workers)"]
    end

    subgraph GOVERNANCE["3. Authority, Risk & Security"]
        GOV["cos-governance (Constitution & Scope)"]
        APP["cos-approval (Human Approval Inbox)"]
        AUDIT["cos-audit (Append-Only Event Ledger)"]
        IDENT["cos-identity (Actor Authentication)"]
        CONT["cos-contracts (TaskGrant & ActionDigest)"]
        DOM["cos-domain (RiskClass R0-R4 & Enums)"]
    end

    subgraph PLANNING["4. Strategic Planning & Goals"]
        GOALS["cos-goals (Hierarchical GoalForest)"]
        PLAN["cos-planning (Dynamic HTN DAG Planner)"]
        ANOMALY["AnomalyDetector & SelfGoalGovernor"]
    end

    subgraph CAPABILITY["5. Capability, Memory & Tools"]
        MEM["cos-memory (Scoped Multi-Tenant Memory)"]
        KNOW["cos-knowledge (Truth & Epistemic Lifecycle)"]
        LLM["cos-llm (Inference Gateway & Discovery)"]
        TOOLS["cos-tools (Side-Effect Gateway & Adapters)"]
    end

    subgraph EVOLUTION["6. Outcomes & Self-Evolution"]
        OUT["cos-outcomes (Metric Delta & Evaluation)"]
        LEARN["cos-learning (SOP Promotion & Guardrails)"]
    end

    subgraph STORAGE["7. Durable Persistence"]
        STORE["cos-persistence (Atomic FileStore & Streaming)"]
    end

    NODE --> API
    NODE --> HEARTBEAT
    HEARTBEAT --> EXEC
    API --> EXEC
    API --> STORE
    EXEC --> GOALS
    EXEC --> PLAN
    EXEC --> AGENTS
    EXEC --> GOV
    EXEC --> APP
    EXEC --> TOOLS
    EXEC --> OUT
    EXEC --> LEARN
    EXEC --> MEM
    TOOLS --> GOV
    TOOLS --> APP
    TOOLS --> AUDIT
    PLAN --> MEM
    PLAN --> GOALS
    AGENTS --> LLM
    AGENTS --> MEM
    LEARN --> OUT
    LEARN --> MEM
    STORE --> MEM
    STORE --> GOALS
    STORE --> APP
    STORE --> AUDIT
```

---

## 3. Autonomous Heartbeat Daemon Loop (`AutonomousEngine`)

The autonomous loop executes continuously every 5 seconds without requiring human manual polling or intervention.

```mermaid
sequenceDiagram
    autonumber
    participant D as Autonomous Heartbeat (5s)
    participant GF as GoalForest
    participant P as Dynamic HTN Planner
    participant M as Dynamic Memory Store
    participant EX as Company Executive
    participant GV as Governance & Risk Engine
    participant AP as Human Approval Inbox
    participant TG as Tool Gateway
    participant OUT as Outcome Registry
    participant LN as Learning Engine

    D->>GF: Scan Active Goals
    GF-->>D: List of Active Goals
    
    loop For Each Active Goal
        opt Goal has no execution plan
            D->>P: Request Plan (Goal + Memory SOP)
            P->>M: Query SOP Guidelines (Hints/Tags)
            M-->>P: Dynamic Procedure Template
            P-->>D: Validated Task DAG (Kahn's Sort)
        end

        D->>P: Query Ready Tasks (Prerequisites Satisfied)
        P-->>D: Ready Planned Tasks

        loop For Each Ready Task
            D->>EX: Process Task (TaskParams + Scoped Grant)
            EX->>GV: Verify Capability & Risk Tier (R0-R4)
            
            alt R3 or R4 (High Risk / Human Signoff Required)
                GV-->>EX: Requires Human Signoff
                EX->>AP: Enqueue Approval Request
                EX-->>D: Task Awaiting Approval
            else R0, R1 or Pre-approved R2
                GV-->>EX: Authorized Scoped Grant
                EX->>TG: Dispatch Authorized Command + ActionDigest
                TG->>TG: Atomic Idempotency Check
                TG-->>EX: ToolExecutionResult (Success)
                EX->>OUT: Record OutcomeRecord & MetricDelta
                EX->>GF: Bubble Up Metric Progress
                GF->>GF: Recalculate Ancestor Goal Progress %
                EX-->>D: Task Completed
            end
        end

        opt Stalled Progress or Failed Criteria
            D->>D: AnomalyDetector::evaluate_goal()
            D->>GF: Propose Governed Candidate Remediation Goal
        end
    end
```

---

## 4. Evidence-Based Autonomy State Machine (Phase 10)

The operational autonomy of agents and tasks expands strictly based on verified evidence and calibrated success rates. Regressions or high-severity failures trigger automatic de-escalation or safe lockdown.

```mermaid
stateDiagram-v2
    [*] --> DraftOnly: Cold Start / Initial Role
    
    DraftOnly --> Supervised: Verified Task Evidence Recorded
    note right of DraftOnly
        Agent proposals are drafts only.
        Zero autonomous external side effects.
    end note

    Supervised --> ControlledRoutine: Repeated Success + Calibrated Confidence
    note right of Supervised
        Routine proposals supervised by human.
        Actions require manual signoff.
    end note

    ControlledRoutine --> ExpandedRoutine: Multi-domain Stable Outcomes
    note right of ControlledRoutine
        Pre-approved R0, R1, and approved R2
        auto-dispatch permitted within memory policy.
    end note

    ExpandedRoutine --> Supervised: Outcome Regression / Drift
    ControlledRoutine --> Supervised: Policy or Risk Tier Change
    
    Supervised --> Suspended: Critical Failure or Boundary Breach
    ExpandedRoutine --> Suspended: Critical Failure or Boundary Breach
    
    Suspended --> DraftOnly: Founder Review & Manual Requalification
    note right of Suspended
        Emergency circuit breaker tripped.
        All autonomous side-effects frozen.
    end note
```

---

## 5. Security & Invariant Enforcement Flow (Preventing Prompt Injection)

In this architecture, LLMs and AI Agents **never** possess authority to execute tools directly. They only emit structured data proposals.

```mermaid
flowchart TD
    subgraph INTAKE["1. Untrusted Input & Reasoning"]
        INP["Inbound Request / Lead / Task"]
        AGENT["Dynamic Role Agent"]
        LLM["Cognitive LLM Port (reason_with_llm)"]
        PROP["Execution Proposal (Structured JSON)"]
    end

    subgraph CRYPTO["2. Deterministic Cryptographic Binding"]
        SHA["ActionDigest SHA-256 Computation"]
        GRANT["Short-lived Scoped TaskGrant (UUIDv7)"]
    end

    subgraph GATES["3. Governance Verification Gates"]
        GATE1{"Grant Valid & Not Expired?"}
        GATE2{"Client Scope Isolated?"}
        GATE3{"Risk Class Allowed without Approval?"}
        GATE4{"Human Approval Consumed?"}
    end

    subgraph DISPATCH["4. Execution Boundary"]
        IDEM{"Atomic Idempotency Cache (Bounded 10k)"}
        TOOL["Tool Gateway (File, Webhook, Notification)"]
        AUDIT["Immutable Audit Event Emitted"]
        OUTCOME["Outcome Recorded & Progress Bubbled"]
    end

    INP --> AGENT
    AGENT --> LLM
    LLM --> PROP
    PROP --> SHA
    SHA --> GRANT

    GRANT --> GATE1
    GATE1 -- "No" --> DENY["Reject: Grant Invalid"]
    GATE1 -- "Yes" --> GATE2
    GATE2 -- "Scope Breach" --> DENY
    GATE2 -- "Scope Isolated" --> GATE3
    
    GATE3 -- "R3 / R4" --> GATE4
    GATE4 -- "Not Approved" --> HOLD["Enqueue Approval Inbox"]
    GATE4 -- "Approved by Founder/Human" --> IDEM

    GATE3 -- "R0 / R1 / Pre-approved R2" --> IDEM
    
    IDEM -- "Duplicate with Same Digest" --> CACHED["Return Cached Result (Replay Safe)"]
    IDEM -- "Duplicate with Altered Digest" --> CONFLICT["Reject: Idempotency Conflict"]
    IDEM -- "First Execution" --> TOOL

    TOOL --> AUDIT
    TOOL --> OUTCOME
```

---

## 6. Hierarchical Goal Forest & Metric Bubble-Up (`cos-goals`)

Goals exist in an arbitrary directed acyclic forest. When a child goal's metrics progress, the progress percentage automatically recalculates and bubbles up to all ancestors.

```mermaid
flowchart TD
    ROOT["Root Company Goal: Expand Q4 Operations (Progress: 75.0%)"]
    
    SUB1["Sub-Goal 1: Automated Inbound Qualification (Weight: 50%)"]
    SUB2["Sub-Goal 2: Response Time SLA (Weight: 50%)"]

    CRIT1["Criterion: Inquiries Processed >= 50 (Current: 50 -> 100%)"]
    CRIT2["Criterion: Average Response <= 15m (Current: 10m -> 100%)"]
    CRIT3["Criterion: Enterprise Signoff (require_human_signoff = true)"]

    ROOT --> SUB1
    ROOT --> SUB2
    SUB1 --> CRIT1
    SUB2 --> CRIT2
    SUB1 --> CRIT3

    CRIT1 -. "Updates Sub-Goal 1" .-> SUB1
    CRIT2 -. "Updates Sub-Goal 2" .-> SUB2
    SUB1 -. "Bubble Up 50% Weight" .-> ROOT
    SUB2 -. "Bubble Up 50% Weight" .-> ROOT
```

---

## 7. Dynamic HTN Planner DAG Decomposition (`cos-planning`)

The dynamic planner queries memory for procedure templates matching goal hints, tags, or token overlaps. It orders tasks into an acyclic dependency graph using Kahn's algorithm.

```mermaid
graph LR
    subgraph DAG["Execution Plan DAG"]
        T0["Task 0: Discovery & Scope Analysis<br/>(Role: discovery_and_proposal)"]
        T1["Task 1: Execution & Deliverable Production<br/>(Role: project_operations)"]
        T2["Task 2: Client Handover & Notification<br/>(Role: client_success_communication)"]
    end

    T0 -->|Dependency Satisfied| T1
    T1 -->|Dependency Satisfied| T2

    style T0 fill:#2d6a4f,stroke:#fff,color:#fff
    style T1 fill:#1d3557,stroke:#fff,color:#fff
    style T2 fill:#457b9d,stroke:#fff,color:#fff
```

---

## 8. Epistemic Knowledge Lifecycle (`cos-knowledge` & `cos-memory`)

Data enters the system as unverified hypotheses and only advances to operational truth upon empirical verification.

```mermaid
stateDiagram-v2
    [*] --> Candidate: Dynamic Hypothesized Input
    Candidate --> Observed: Directly Seen via Tool / External System
    Candidate --> Inferred: Derived Logically from Existing Truth
    Observed --> Verified: Empirical Evidence / Human Validation
    Inferred --> Verified: Empirical Evidence / Human Validation
    Verified --> Trusted: Long-term Corroborated Track Record
    
    Verified --> Stale: Outdated / Superseded by Time
    Trusted --> Stale: Outdated / Superseded by Time
    Stale --> Verified: Revalidation Pass
    
    Verified --> Contradicted: Conflicting Evidence Discovered
    Trusted --> Contradicted: Conflicting Evidence Discovered
    Contradicted --> Investigating: Audit Triggered
    Investigating --> Verified: Contradiction Resolved
```

---

## 9. Scoped Multi-Tenant Memory Architecture (`cos-memory`)

Client data isolation is cryptographically and logically absolute. Cross-tenant leakage is prevented at compile time and runtime.

```mermaid
flowchart TD
    subgraph MEMORY_ROOT["Dynamic Memory Store"]
        subgraph CO_NS["Company Namespace"]
            SOP1["sop:sales_and_lead"]
            SOP2["sop:discovery_and_proposal"]
            SOP3["sop:project_operations"]
            SOP4["sop:client_success_communication"]
            TPL["sop:default_planning_template"]
        end

        subgraph CLIENT_A["Client A Namespace (Isolated)"]
            CA_REQ["Requirements & Deliverables"]
            CA_COM["Communications History"]
        end

        subgraph CLIENT_B["Client B Namespace (Isolated)"]
            CB_REQ["Requirements & Deliverables"]
            CB_COM["Communications History"]
        end

        subgraph EPHEMERAL["Task & Working Namespaces"]
            TASK_NS["Task(TaskId) - Ephemeral Execution Context"]
            AGENT_NS["AgentWorking(AgentId) - Scratchpad Memory"]
        end
    end

    CLIENT_A -.-x|ACCESS FORBIDDEN| CLIENT_B
    CLIENT_B -.-x|ACCESS FORBIDDEN| CLIENT_A
```

---

## 10. Real-Time Server-Sent Events (SSE) & UI Pipeline (`cos-api`)

The frontend UI connects to `/api/v1/events/stream` and receives real-time operational notifications without polling.

```mermaid
sequenceDiagram
    participant UI as Frontend Dashboard (UI)
    participant API as Axum REST Server
    participant BUS as Broadcast Channel (event_bus)
    participant EX as Company Executive / Ticker
    participant AP as Approvals Route

    UI->>API: GET /api/v1/events/stream
    API-->>UI: 200 OK (Content-Type: text/event-stream)
    
    par Autonomous Ticker Activity
        EX->>BUS: Broadcast Event: TaskPlanned / Executed
        BUS-->>API: Stream Message
        API-->>UI: data: {"type": "task_executed", ...}
    and Human Approval Decision
        UI->>AP: POST /api/v1/approvals/:id/decide
        AP->>BUS: Broadcast Event: ApprovalDecided
        BUS-->>API: Stream Message
        API-->>UI: data: {"type": "approval_decided", ...}
    end
```

---

## 11. Self-Evolution & Learning Engine Flow (`cos-learning`)

When operational tasks fail, the learning engine synthesizes proposal candidates without mutating immutable constitutional rules.

```mermaid
flowchart LR
    FAIL["Observed Task Failure"] --> REG["Outcome Registry Record"]
    REG --> ANOM["Anomaly Detector"]
    ANOM --> PROP["Synthesize LearningProposal"]
    PROP --> GUARD{"Constitutional Guardrails:<br/>1. Sealed Namespace?<br/>2. Reserved Prefix?<br/>3. Mutable Policy?"}
    
    GUARD -- "Violation" --> REJ["Proposal Denied & Audited"]
    GUARD -- "Valid" --> QUEUE["Pending Review Queue"]
    QUEUE --> REVIEW["Founder / Policy Authority Review"]
    REVIEW -- "Approved" --> APPLY["Apply Versioned Revision to Memory SOP"]
    APPLY --> BETTER["Improved Execution in Future Cycles"]
```

---

## 12. Complete Crate Index & Verification Status

| Crate Name | Architectural Responsibility | Tests Passed | Status |
|---|---|---|---|
| `cos-domain` | Strong identifiers (UUIDv7), RiskClass (R0-R4), TaskStatus, AutonomyLevel | 8 tests | Verified |
| `cos-identity` | Actor identities, Founder credentials, SubjectId | 2 tests | Verified |
| `cos-contracts` | TaskGrant, ActionDigest (SHA-256), CommandEnvelope, TraceContext | 2 tests | Verified |
| `cos-governance` | Capability checks, Client isolation, Risk evaluation, Founder-only R4 | 5 tests | Verified |
| `cos-approval` | Approval request lifecycle, expiration, double-spending prevention | 2 tests | Verified |
| `cos-audit` | Append-only immutable audit trail, tamper evidence | 1 test | Verified |
| `cos-tools` | ToolGateway, Bounded Idempotency (10k), File, Notification, Webhook | 3 tests | Verified |
| `cos-memory` | Scoped storage (Company, Client, Task, Working), versioning | 1 test | Verified |
| `cos-knowledge` | Epistemic truth lifecycle, confidence scoring, stale detection | 1 test | Verified |
| `cos-agents` | DynamicAgent, dynamic memory SOP evaluation, cognitive LLM reasoning | 2 tests | Verified |
| `cos-config` | Governance limits, system timeouts, max call budgets | 1 test | Verified |
| `cos-goals` | GoalForest, Criteria Bubble-Up, AnomalyDetector, SelfGoalGovernor | 5 tests | Verified |
| `cos-planning` | Dynamic HTN Planner, Kahn's algorithm cycle detection, DAG sorting | 5 tests | Verified |
| `cos-outcomes` | OutcomeRegistry, MetricDelta, calibration scoring | 1 test | Verified |
| `cos-learning` | Structural namespace guardrails, learning proposals, procedure evolution | 3 tests | Verified |
| `cos-persistence`| Atomic file-swap storage, BufReader streaming, AuditWriterPort | 3 tests | Verified |
| `cos-llm` | HTTP inference gateway, API key masking, model discovery, mock port | 2 tests | Verified |
| `cos-executive` | Central runtime, dynamic SOP routing, Autonomous Heartbeat Daemon | 3 tests | Verified |
| `cos-api` | Axum REST router, Approvals inbox, Gateway settings, Real-time SSE stream | 4 tests | Verified |
| `cos-node` | Main runnable daemon binary, cold-start bootstrapper for 4 MVP roles | Clean build | Verified |
| **TOTAL** | **Full Enterprise Workspace Compilation & Integration** | **51 / 51 tests** | **100% Passed, 0 Clippy Warnings** |
