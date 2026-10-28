use serde::{Deserialize, Serialize};
use std::fmt;

/// Canonical Risk Classes defining execution boundaries, impact levels, and approval mandates.
///
/// Fundamental Law: Agent confidence does NOT reduce risk class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskClass {
    /// R0: Internal Read-only actions (CRM reads, analytics lookups, draft generation).
    /// Execution: Automatic with valid capability grant.
    R0InternalRead,

    /// R1: Reversible low-impact actions (internal task creation, approved template reminder).
    /// Execution: Automatic after governance policy check.
    R1ReversibleLowImpact,

    /// R2: External communication (outbound client messages, scheduled routine content).
    /// Execution: Approval required by default; policy may allow pre-approved templates and scopes.
    R2ExternalCommunication,

    /// R3: Business commitment (pricing quotes, discounts, timeline commitments, scope promises).
    /// Execution: Strict human/authorized approver decision required.
    R3BusinessCommitment,

    /// R4: Irreversible financial, legal, or public claims (contracts, refunds, payments, public broadcast).
    /// Execution: Explicit founder approval mandatory; delegation strictly limited by constitution.
    R4IrreversibleCritical,
}

impl RiskClass {
    /// Returns true if this risk class requires explicit human approval by default.
    #[inline]
    pub const fn requires_human_approval(&self) -> bool {
        matches!(
            self,
            Self::R3BusinessCommitment | Self::R4IrreversibleCritical
        )
    }

    /// Returns true if this action is strictly founder-only and cannot be delegated to operational policy.
    #[inline]
    pub const fn is_founder_only(&self) -> bool {
        matches!(self, Self::R4IrreversibleCritical)
    }

    /// Returns true if this action can be executed automatically under a verified policy.
    #[inline]
    pub const fn allows_automatic_execution(&self) -> bool {
        matches!(self, Self::R0InternalRead | Self::R1ReversibleLowImpact)
    }

    /// Returns the risk tier level as an integer from 0 to 4.
    #[inline]
    pub const fn level(&self) -> u8 {
        match self {
            Self::R0InternalRead => 0,
            Self::R1ReversibleLowImpact => 1,
            Self::R2ExternalCommunication => 2,
            Self::R3BusinessCommitment => 3,
            Self::R4IrreversibleCritical => 4,
        }
    }
}

impl fmt::Display for RiskClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::R0InternalRead => write!(f, "R0_INTERNAL_READ"),
            Self::R1ReversibleLowImpact => write!(f, "R1_REVERSIBLE_LOW_IMPACT"),
            Self::R2ExternalCommunication => write!(f, "R2_EXTERNAL_COMMUNICATION"),
            Self::R3BusinessCommitment => write!(f, "R3_BUSINESS_COMMITMENT"),
            Self::R4IrreversibleCritical => write!(f, "R4_IRREVERSIBLE_CRITICAL"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_class_hierarchy() {
        assert!(RiskClass::R0InternalRead < RiskClass::R1ReversibleLowImpact);
        assert!(RiskClass::R1ReversibleLowImpact < RiskClass::R2ExternalCommunication);
        assert!(RiskClass::R2ExternalCommunication < RiskClass::R3BusinessCommitment);
        assert!(RiskClass::R3BusinessCommitment < RiskClass::R4IrreversibleCritical);
    }

    #[test]
    fn test_risk_approval_mandates() {
        assert!(!RiskClass::R0InternalRead.requires_human_approval());
        assert!(!RiskClass::R1ReversibleLowImpact.requires_human_approval());
        assert!(RiskClass::R3BusinessCommitment.requires_human_approval());
        assert!(RiskClass::R4IrreversibleCritical.requires_human_approval());
        assert!(RiskClass::R4IrreversibleCritical.is_founder_only());
        assert!(!RiskClass::R3BusinessCommitment.is_founder_only());
    }
}
