//! `cos-config` provides typed configuration loading for generic system limits and auth providers.
//!
//! # Architecture Invariant:
//! Zero business strings, business templates, or business thresholds exist here.
//! All business knowledge, SOPs, and guidelines belong exclusively in MemoryRecords.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read configuration file at '{path}': {source}")]
    IoError {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to parse TOML configuration: {0}")]
    ParseError(#[from] toml::de::Error),
}

/// Governance timeouts, call limits, and policy versioning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceLimitsConfig {
    pub default_grant_duration_minutes: i64,
    pub default_approval_duration_hours: i64,
    pub default_grant_max_calls: u32,
    pub policy_version: u32,
}

impl Default for GovernanceLimitsConfig {
    fn default() -> Self {
        Self {
            default_grant_duration_minutes: 15,
            default_approval_duration_hours: 24,
            default_grant_max_calls: 3,
            policy_version: 1,
        }
    }
}

/// Auth provider names and credential URI schemes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthProvidersConfig {
    pub system_provider: String,
    pub executive_provider: String,
    pub founder_provider: String,
    pub agent_provider: String,
    pub system_credential_prefix: String,
    pub executive_credential_uri: String,
    pub founder_credential_uri: String,
    pub agent_credential_prefix: String,
}

impl Default for AuthProvidersConfig {
    fn default() -> Self {
        Self {
            system_provider: "system_internal".to_string(),
            executive_provider: "executive_internal".to_string(),
            founder_provider: "founder_auth_provider".to_string(),
            agent_provider: "runtime_agent_manager".to_string(),
            system_credential_prefix: "system://".to_string(),
            executive_credential_uri: "system://executive".to_string(),
            founder_credential_uri: "auth://founder_credential".to_string(),
            agent_credential_prefix: "agent://".to_string(),
        }
    }
}

/// Top-level application configuration for operational infrastructure.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub governance: GovernanceLimitsConfig,
    pub auth_providers: AuthProvidersConfig,
}

impl AppConfig {
    /// Loads configuration from a TOML string.
    pub fn from_toml_str(toml_str: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(toml_str)?;
        Ok(config)
    }

    /// Loads configuration from a file on disk.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let p = path.as_ref();
        let contents = fs::read_to_string(p).map_err(|source| ConfigError::IoError {
            path: p.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_serialization() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).expect("Serialization failed");
        let parsed = AppConfig::from_toml_str(&toml_str).expect("Parsing failed");
        assert_eq!(config, parsed);
    }
}
