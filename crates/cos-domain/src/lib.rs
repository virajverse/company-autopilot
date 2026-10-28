//! `cos-domain` defines the core domain entities, value objects, identifiers,
//! risk classes, and invariant-enforcing state machines for the Autonomous Company Operating System.
//!
//! # Architecture Rules:
//! - Strictly Zero I/O, zero network, zero SQL, zero LLM provider dependencies.
//! - Completely brand-agnostic and white-label.
//! - Compile-time typing of authority boundaries and risk tiers.

pub mod error;
pub mod identifiers;
pub mod lifecycle;
pub mod risk;

// Re-export common domain types for clean usage across crates
pub use error::DomainError;
pub use identifiers::{
    AgentId, ApprovalId, CausationId, ClientScope, CorrelationId, GoalId, GrantId, MemoryId,
    OrganizationId, ProjectScope, SubjectId, TaskId, TenantId, TraceId,
};
pub use lifecycle::{ApprovalStatus, AutonomyLevel, GrantStatus, KnowledgeLifecycle, TaskStatus};
pub use risk::RiskClass;
