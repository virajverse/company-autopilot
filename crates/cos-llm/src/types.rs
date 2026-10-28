use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Method used to provide gateway credentials.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// Static API key provided directly.
    #[default]
    StaticApiKey,
    /// Loaded from environment variable.
    EnvironmentVariable,
    /// AWS IAM / SigV4 Role.
    AwsIamRole,
    /// OIDC identity token.
    OidcToken,
}

/// How the gateway credential is sent on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayAuthScheme {
    /// Standard `Authorization: Bearer <key>` header.
    #[default]
    Bearer,
    /// `x-api-key: <key>` header (Anthropic / AWS API Gateway style).
    XApiKey,
    /// Custom header name specified by the user.
    CustomHeader(String),
}

/// Individual model configuration entry (1-to-1 match with UI settings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntryConfig {
    /// Identifier sent to the inference endpoint (e.g. "claude-sonnet-4-5").
    pub model_id: String,
    /// Display name shown in UI dropdowns/pickers (blank = auto-format from ID).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Adds a 1M-context variant of this model to picker.
    #[serde(default)]
    pub offer_1m_variant: bool,
    /// Shows one option for this model and requests 1M from provider.
    #[serde(default)]
    pub only_1m_option: bool,
    /// Whether this entry is marked as the default model.
    #[serde(default)]
    pub is_default: bool,
}

impl ModelEntryConfig {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            display_name: None,
            offer_1m_variant: false,
            only_1m_option: false,
            is_default: false,
        }
    }
}

/// Models discovery and selection configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelsConfig {
    /// Auto-populate model picker from `{gateway_base_url}/v1/models` at launch.
    pub model_discovery_enabled: bool,
    /// Every model that offers 1M context requests 1M.
    pub prefer_1m_context: bool,
    /// Explicit list of models (first entry is default if not specified).
    pub model_list: Vec<ModelEntryConfig>,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            model_discovery_enabled: true,
            prefer_1m_context: false,
            model_list: Vec::new(),
        }
    }
}

/// Full gateway configuration matching inference gateway settings UI.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmGatewayConfig {
    /// Credential source kind.
    pub credential_kind: CredentialKind,
    /// Full base URL of the inference gateway endpoint (e.g. "http://127.0.0.1:8008").
    pub gateway_base_url: String,
    /// Gateway secret API key or token.
    pub gateway_api_key: String,
    /// Header authentication scheme (Bearer vs x-api-key).
    pub gateway_auth_scheme: GatewayAuthScheme,
    /// Optional HTTPS origin of user-content-renderer deployment for artifact previews.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_preview_iframe_origin: Option<String>,
    /// Extra headers sent on every inference request (routing, tenant headers, Bedrock guardrails).
    #[serde(default)]
    pub custom_inference_headers: HashMap<String, String>,
    /// Extra seconds to wait for model output on a streaming response (default 300).
    pub stream_idle_timeout_seconds: u32,
    /// When true, sessions signed in through gateway run Claude Code in gateway mode.
    pub gateway_managed_claude_code: bool,
    /// Model discovery and model registry settings.
    pub models: ModelsConfig,
    /// Optional override for the chat completions endpoint path (defaults to "/v1/chat/completions").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_endpoint_override: Option<String>,
    /// Optional override for the models discovery endpoint path (defaults to "/v1/models").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_endpoint_override: Option<String>,
}

impl Default for LlmGatewayConfig {
    fn default() -> Self {
        Self {
            credential_kind: CredentialKind::StaticApiKey,
            gateway_base_url: std::env::var("COS_LLM_BASE_URL").unwrap_or_default(),
            gateway_api_key: String::new(),
            gateway_auth_scheme: GatewayAuthScheme::Bearer,
            artifact_preview_iframe_origin: None,
            custom_inference_headers: HashMap::new(),
            stream_idle_timeout_seconds: 300,
            gateway_managed_claude_code: false,
            models: ModelsConfig::default(),
            chat_endpoint_override: None,
            models_endpoint_override: None,
        }
    }
}

impl LlmGatewayConfig {
    /// Generates a UI-safe sanitized copy of the configuration (masks API key).
    pub fn to_ui_safe(&self) -> Self {
        let masked_key = if self.gateway_api_key.len() <= 6 {
            "******".to_string()
        } else {
            let prefix = &self.gateway_api_key[..3];
            let suffix = &self.gateway_api_key[self.gateway_api_key.len() - 3..];
            format!("{}••••••••{}", prefix, suffix)
        };

        Self {
            gateway_api_key: masked_key,
            ..self.clone()
        }
    }
}

impl std::fmt::Debug for LlmGatewayConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let safe = self.to_ui_safe();
        f.debug_struct("LlmGatewayConfig")
            .field("credential_kind", &self.credential_kind)
            .field("gateway_base_url", &self.gateway_base_url)
            .field("gateway_api_key", &safe.gateway_api_key)
            .field("gateway_auth_scheme", &self.gateway_auth_scheme)
            .field(
                "artifact_preview_iframe_origin",
                &self.artifact_preview_iframe_origin,
            )
            .field("custom_inference_headers", &self.custom_inference_headers)
            .field(
                "stream_idle_timeout_seconds",
                &self.stream_idle_timeout_seconds,
            )
            .field(
                "gateway_managed_claude_code",
                &self.gateway_managed_claude_code,
            )
            .field("models", &self.models)
            .field("chat_endpoint_override", &self.chat_endpoint_override)
            .field("models_endpoint_override", &self.models_endpoint_override)
            .finish()
    }
}

/// Result returned from gateway connection verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionVerifyResult {
    pub success: bool,
    pub latency_ms: u64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_server: Option<String>,
}

pub type ConnectionTestResult = ConnectionVerifyResult;

/// Model discovered from `/v1/models` endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredModel {
    pub id: String,
    pub object: Option<String>,
    pub created: Option<i64>,
    pub owned_by: Option<String>,
}

/// Response returned from `/v1/models` standard OpenAI/compatible format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiModelsResponse {
    pub data: Vec<DiscoveredModel>,
}
