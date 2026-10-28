use crate::port::{CompletionRequest, CompletionResponse, LlmError, LlmPort};
use crate::types::{ConnectionVerifyResult, DiscoveredModel};

/// High-integrity fallback representing an unconfigured LLM inference gateway.
///
/// Production Invariant:
/// Rather than returning synthetic passes, any invocation against
/// an unconfigured gateway cleanly and explicitly returns an `LlmError::Configuration`
/// directing the operator to supply valid model endpoints and API credentials.
#[derive(Clone, Debug, Default)]
pub struct UnconfiguredLlmGateway {
    pub reason: String,
}

impl UnconfiguredLlmGateway {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait::async_trait]
impl LlmPort for UnconfiguredLlmGateway {
    async fn verify_connection(&self) -> Result<ConnectionVerifyResult, LlmError> {
        let msg = if self.reason.is_empty() {
            "Inference gateway is unconfigured. Please provide an API key and base URL.".to_string()
        } else {
            format!("Inference gateway configuration failed: {}", self.reason)
        };

        Ok(ConnectionVerifyResult {
            success: false,
            latency_ms: 0,
            message: msg,
            detected_server: None,
        })
    }

    async fn discover_models(&self) -> Result<Vec<DiscoveredModel>, LlmError> {
        Err(LlmError::Configuration(format!(
            "Model discovery unavailable: gateway is unconfigured ({})",
            self.reason
        )))
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(LlmError::Configuration(format!(
            "Cannot execute chat completion: inference gateway is unconfigured ({})",
            self.reason
        )))
    }
}
