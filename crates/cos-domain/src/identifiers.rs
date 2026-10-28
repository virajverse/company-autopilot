use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::DomainError;

macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Generates a new random UUID v4 identifier.
            pub fn new_v4() -> Self {
                Self(Uuid::new_v4())
            }

            /// Generates a new time-ordered UUID v7 identifier.
            pub fn new_v7() -> Self {
                Self(Uuid::now_v7())
            }

            /// Creates an identifier wrapping an existing UUID.
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns the inner UUID.
            pub const fn into_inner(self) -> Uuid {
                self.0
            }

            /// Returns a reference to the inner UUID.
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = DomainError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self).map_err(|e| {
                    DomainError::InvalidIdentifier(format!("{}: {}", stringify!($name), e))
                })
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }
    };
}

define_id!(TenantId, "Identifies the tenant/company boundary.");
define_id!(
    OrganizationId,
    "Identifies an organization boundary within a tenant."
);
define_id!(
    SubjectId,
    "Identifies an authenticated actor (human founder, executive, agent, external caller)."
);
define_id!(
    GoalId,
    "Identifies a strategic or operational goal in the goal forest."
);
define_id!(TaskId, "Identifies a scoped execution task.");
define_id!(AgentId, "Identifies an instantiated agent worker.");
define_id!(GrantId, "Identifies a short-lived capability grant.");
define_id!(ApprovalId, "Identifies a human or policy approval request.");
define_id!(MemoryId, "Identifies a persisted memory record.");
define_id!(
    TraceId,
    "Identifies a distributed causal trace across operations."
);
define_id!(
    CorrelationId,
    "Identifies a broad business workflow across multiple operations."
);
define_id!(
    CausationId,
    "Identifies the immediate direct parent operation that triggered this action."
);
define_id!(
    ClientScope,
    "Identifies a specific client data namespace boundary."
);
define_id!(
    ProjectScope,
    "Identifies a specific project namespace boundary within a client."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_generation_and_string_roundtrip() {
        let tenant_id = TenantId::new_v7();
        let s = tenant_id.to_string();
        let parsed: TenantId = s.parse().expect("Failed to parse TenantId");
        assert_eq!(tenant_id, parsed);
    }

    #[test]
    fn test_serde_json_roundtrip() {
        let task_id = TaskId::new_v4();
        let json = serde_json::to_string(&task_id).expect("Serialization failed");
        let deserialized: TaskId = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(task_id, deserialized);
    }
}
