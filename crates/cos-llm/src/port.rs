use crate::types::{ConnectionVerifyResult, DiscoveredModel};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum LlmError {
    #[error("Network error communicating with LLM gateway: {0}")]
    Network(String),

    #[error("Authentication failed with LLM gateway: {0}")]
    Authentication(String),

    #[error("Gateway timed out after {0} seconds")]
    Timeout(u32),

    #[error("Requested model '{0}' was not found on the gateway")]
    ModelNotFound(String),

    #[error("Provider error HTTP {status}: {message}")]
    Provider { status: u16, message: String },

    #[error("Serialization / parsing error: {0}")]
    Serialization(String),

    #[error("Configuration error: {0}")]
    Configuration(String),
}

/// Message role in conversational LLM interactions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Individual chat message sent to or received from the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
            name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            name: None,
        }
    }
}

/// Token usage statistics from an inference request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Request sent to the LLM Gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Specific model to invoke. If None, gateway uses its default model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Chronological list of chat messages.
    pub messages: Vec<ChatMessage>,
    /// Sampling temperature (0.0 to 2.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Maximum output tokens to generate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Whether to instruct the model to return valid JSON.
    #[serde(default)]
    pub response_format_json: bool,
}

impl CompletionRequest {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        Self {
            model: None,
            messages,
            temperature: None,
            max_tokens: None,
            response_format_json: false,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    pub fn json_mode(mut self) -> Self {
        self.response_format_json = true;
        self
    }
}

/// Response returned from an LLM inference completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Model name that performed the inference.
    pub model: String,
    /// Generated text content.
    pub content: String,
    /// Finish reason (e.g. "stop", "length", "tool_calls").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Token usage metrics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

/// Core port trait implemented by inference providers and cloud gateways.
#[async_trait::async_trait]
pub trait LlmPort: Send + Sync {
    /// Verifies connection and latency to the configured cloud gateway.
    async fn verify_connection(&self) -> Result<ConnectionVerifyResult, LlmError>;

    /// Backwards-compatible alias for verify_connection.
    async fn test_connection(&self) -> Result<ConnectionVerifyResult, LlmError> {
        self.verify_connection().await
    }

    /// Dynamically discovers available models from the inference gateway endpoint.
    async fn discover_models(&self) -> Result<Vec<DiscoveredModel>, LlmError>;

    /// Executes a standard or structured chat completion.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// Generates vector embeddings for semantic search on CPU.
    /// Default implementation returns a deterministic normalized vector based on SHA-256 hash.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        use sha2::Digest;
        let hash = sha2::Sha256::digest(text.as_bytes());
        let mut vec = Vec::with_capacity(16);
        for chunk in hash.as_chunks::<2>().0 {
            let val = ((chunk[0] as f32 * 256.0 + chunk[1] as f32) / 65535.0) * 2.0 - 1.0;
            vec.push(val);
        }
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        Ok(vec)
    }
}
