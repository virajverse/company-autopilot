use thiserror::Error;

/// Domain errors representing invariant violations, invalid state transitions, or boundary violations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("Invalid state transition from '{from}' to '{to}': {reason}")]
    InvalidStateTransition {
        from: String,
        to: String,
        reason: String,
    },

    #[error("Invariant violation: {0}")]
    InvariantViolation(String),

    #[error("Scope mismatch: expected scope '{expected}', found '{found}'")]
    ScopeMismatch { expected: String, found: String },

    #[error("Unauthorized action attempt: {0}")]
    UnauthorizedAction(String),

    #[error("Invalid identifier: {0}")]
    InvalidIdentifier(String),
}
