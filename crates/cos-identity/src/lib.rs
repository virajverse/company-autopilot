//! `cos-identity` defines canonical actor identities, categories, opaque credential references,
//! and authentication boundaries.
//!
//! # Security Invariant:
//! Raw passwords, API keys, tokens, or provider secrets are NEVER stored or passed inside ActorIdentity.
//! All credentials are held exclusively by the respective Credential Stores and referenced via opaque keys.

use chrono::{DateTime, Utc};
use cos_config::AuthProvidersConfig;
use cos_domain::SubjectId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// High-level categorization of actors within the operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActorCategory {
    /// The primary human founder / system owner with highest constitutional authority.
    Founder,
    /// A designated human approver with scoped decision-making authority.
    NamedHumanApprover,
    /// The central coordination engine of the company operating system.
    CompanyExecutive,
    /// An instantiated, specialized role-based agent executing scoped tasks.
    RoleAgent,
    /// An internal platform background service (e.g., scheduler, audit writer, outbox relay).
    SystemService,
    /// An external caller or inbound webhook source (untrusted until verified).
    ExternalSource,
}

impl ActorCategory {
    #[inline]
    pub const fn is_human(&self) -> bool {
        matches!(self, Self::Founder | Self::NamedHumanApprover)
    }

    #[inline]
    pub const fn is_founder(&self) -> bool {
        matches!(self, Self::Founder)
    }

    #[inline]
    pub const fn is_agent(&self) -> bool {
        matches!(self, Self::RoleAgent)
    }

    #[inline]
    pub const fn is_executive(&self) -> bool {
        matches!(self, Self::CompanyExecutive)
    }
}

/// Opaque reference to a credential stored securely in a separate Credential Store.
///
/// Invariant: Never contains raw keys, passwords, or secret material.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CredentialRef(pub String);

impl CredentialRef {
    pub fn new(reference: impl Into<String>) -> Self {
        Self(reference.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque reference identifying the authentication provider (e.g. "local_msvc", "jwt_oidc", "system_internal").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthProviderRef(pub String);

impl AuthProviderRef {
    pub fn new(provider: impl Into<String>) -> Self {
        Self(provider.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque reference identifying an active authenticated session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionRef(pub String);

impl SessionRef {
    pub fn new(session: impl Into<String>) -> Self {
        Self(session.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Canonical authenticated actor identity carried across every command, event, approval, and audit record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorIdentity {
    pub subject_id: SubjectId,
    pub actor_category: ActorCategory,
    pub credential_ref: CredentialRef,
    pub provider_ref: AuthProviderRef,
    pub authenticated_at: DateTime<Utc>,
    pub session_ref: Option<SessionRef>,
}

impl ActorIdentity {
    /// Creates a system-internal service identity using configuration.
    pub fn system_with_config(name: &str, config: &AuthProvidersConfig) -> Self {
        Self {
            subject_id: SubjectId::new_v7(),
            actor_category: ActorCategory::SystemService,
            credential_ref: CredentialRef::new(format!(
                "{}{}",
                config.system_credential_prefix, name
            )),
            provider_ref: AuthProviderRef::new(&config.system_provider),
            authenticated_at: Utc::now(),
            session_ref: None,
        }
    }

    /// Creates a system-internal service identity with default configuration.
    pub fn system(name: &'static str) -> Self {
        Self::system_with_config(name, &AuthProvidersConfig::default())
    }

    /// Creates an executive identity using configuration.
    pub fn executive_with_config(subject_id: SubjectId, config: &AuthProvidersConfig) -> Self {
        Self {
            subject_id,
            actor_category: ActorCategory::CompanyExecutive,
            credential_ref: CredentialRef::new(&config.executive_credential_uri),
            provider_ref: AuthProviderRef::new(&config.executive_provider),
            authenticated_at: Utc::now(),
            session_ref: None,
        }
    }

    /// Creates an executive identity with default configuration.
    pub fn executive(subject_id: SubjectId) -> Self {
        Self::executive_with_config(subject_id, &AuthProvidersConfig::default())
    }

    /// Creates a founder identity using configuration.
    pub fn founder_with_config(
        subject_id: SubjectId,
        session: Option<SessionRef>,
        config: &AuthProvidersConfig,
    ) -> Self {
        Self {
            subject_id,
            actor_category: ActorCategory::Founder,
            credential_ref: CredentialRef::new(&config.founder_credential_uri),
            provider_ref: AuthProviderRef::new(&config.founder_provider),
            authenticated_at: Utc::now(),
            session_ref: session,
        }
    }

    /// Creates a founder identity with default configuration.
    pub fn founder(subject_id: SubjectId, session: Option<SessionRef>) -> Self {
        Self::founder_with_config(subject_id, session, &AuthProvidersConfig::default())
    }

    /// Creates an agent identity using configuration.
    pub fn agent_with_config(
        subject_id: SubjectId,
        role_name: &str,
        config: &AuthProvidersConfig,
    ) -> Self {
        Self {
            subject_id,
            actor_category: ActorCategory::RoleAgent,
            credential_ref: CredentialRef::new(format!(
                "{}{}",
                config.agent_credential_prefix, role_name
            )),
            provider_ref: AuthProviderRef::new(&config.agent_provider),
            authenticated_at: Utc::now(),
            session_ref: None,
        }
    }

    /// Creates an agent identity with default configuration.
    pub fn agent(subject_id: SubjectId, role_name: &str) -> Self {
        Self::agent_with_config(subject_id, role_name, &AuthProvidersConfig::default())
    }
}

impl fmt::Display for ActorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_identity_creation_and_serialization() {
        let founder =
            ActorIdentity::founder(SubjectId::new_v7(), Some(SessionRef::new("sess_123")));
        assert!(founder.actor_category.is_founder());
        assert!(founder.actor_category.is_human());
        assert!(!founder.actor_category.is_agent());

        let json = serde_json::to_string(&founder).expect("Failed to serialize ActorIdentity");
        let deserialized: ActorIdentity =
            serde_json::from_str(&json).expect("Failed to deserialize ActorIdentity");
        assert_eq!(founder, deserialized);
    }

    #[test]
    fn test_system_and_agent_identities() {
        let sys = ActorIdentity::system("scheduler");
        assert_eq!(sys.actor_category, ActorCategory::SystemService);
        assert!(!sys.actor_category.is_human());

        let agent = ActorIdentity::agent(SubjectId::new_v7(), "sales");
        assert!(agent.actor_category.is_agent());
    }
}
