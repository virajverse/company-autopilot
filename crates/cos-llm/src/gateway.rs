use crate::port::{
    ChatMessage, CompletionRequest, CompletionResponse, LlmError, LlmPort, TokenUsage,
};
use crate::types::{
    ConnectionVerifyResult, DiscoveredModel, GatewayAuthScheme, LlmGatewayConfig,
    OpenAiModelsResponse,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::{Duration, Instant};

/// HTTP-based implementation of `LlmPort` supporting standard inference gateways.
#[derive(Debug, Clone)]
pub struct HttpInferenceGateway {
    config: LlmGatewayConfig,
    client: reqwest::Client,
}

impl HttpInferenceGateway {
    /// Creates a new HTTP inference gateway from the provided configuration.
    pub fn new(config: LlmGatewayConfig) -> Result<Self, LlmError> {
        let timeout_secs = if config.stream_idle_timeout_seconds == 0 {
            300
        } else {
            config.stream_idle_timeout_seconds
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs as u64))
            .build()
            .map_err(|e| LlmError::Configuration(e.to_string()))?;

        Ok(Self { config, client })
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &LlmGatewayConfig {
        &self.config
    }

    /// Constructs HTTP headers according to gateway auth scheme and custom headers.
    fn build_headers(&self) -> Result<HeaderMap, LlmError> {
        let mut headers = HeaderMap::new();

        // 1. Authentication header
        match &self.config.gateway_auth_scheme {
            GatewayAuthScheme::Bearer => {
                let auth_val = format!("Bearer {}", self.config.gateway_api_key);
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&auth_val)
                        .map_err(|e| LlmError::Configuration(e.to_string()))?,
                );
            }
            GatewayAuthScheme::XApiKey => {
                headers.insert(
                    HeaderName::from_static("x-api-key"),
                    HeaderValue::from_str(&self.config.gateway_api_key)
                        .map_err(|e| LlmError::Configuration(e.to_string()))?,
                );
            }
            GatewayAuthScheme::CustomHeader(name) => {
                let header_name = HeaderName::from_str(name)
                    .map_err(|e| LlmError::Configuration(e.to_string()))?;
                headers.insert(
                    header_name,
                    HeaderValue::from_str(&self.config.gateway_api_key)
                        .map_err(|e| LlmError::Configuration(e.to_string()))?,
                );
            }
        }

        // 2. Custom inference headers
        for (k, v) in &self.config.custom_inference_headers {
            let header_name = HeaderName::from_str(k).map_err(|e| {
                LlmError::Configuration(format!("Invalid custom header name '{}': {}", k, e))
            })?;
            let header_val = HeaderValue::from_str(v).map_err(|e| {
                LlmError::Configuration(format!("Invalid custom header value for '{}': {}", k, e))
            })?;
            headers.insert(header_name, header_val);
        }

        Ok(headers)
    }

    /// Selects the effective model ID for an inference request.
    fn resolve_model_id(&self, requested: Option<&str>) -> String {
        if let Some(m) = requested {
            return m.to_string();
        }

        // Check if there is an explicit default or first entry in the configured model list
        if let Some(first) = self.config.models.model_list.iter().find(|m| m.is_default) {
            return first.model_id.clone();
        }
        if let Some(first) = self.config.models.model_list.first() {
            return first.model_id.clone();
        }

        "default".to_string()
    }
}

#[derive(Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormatWrapper>,
}

#[derive(Serialize)]
struct ResponseFormatWrapper {
    #[serde(rename = "type")]
    format_type: &'static str,
}

#[derive(Deserialize)]
struct OpenAiChatChoice {
    message: ChatMessageOutput,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatMessageOutput {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsageOutput {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    model: Option<String>,
    choices: Vec<OpenAiChatChoice>,
    usage: Option<OpenAiUsageOutput>,
}

#[async_trait::async_trait]
impl LlmPort for HttpInferenceGateway {
    async fn verify_connection(&self) -> Result<ConnectionVerifyResult, LlmError> {
        let base = self.config.gateway_base_url.trim_end_matches('/');
        let endpoint = self
            .config
            .models_endpoint_override
            .as_deref()
            .unwrap_or("/v1/models");
        let url = format!("{}{}", base, endpoint);
        let headers = self.build_headers()?;

        let start = Instant::now();
        let res = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        let latency_ms = start.elapsed().as_millis() as u64;

        if res.status().is_success() {
            let server_hdr = res
                .headers()
                .get("server")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            Ok(ConnectionVerifyResult {
                success: true,
                latency_ms,
                message: format!(
                    "Successfully reached gateway endpoint: HTTP {}",
                    res.status()
                ),
                detected_server: server_hdr,
            })
        } else {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            Ok(ConnectionVerifyResult {
                success: false,
                latency_ms,
                message: format!("Gateway returned status {}: {}", status, body),
                detected_server: None,
            })
        }
    }

    async fn discover_models(&self) -> Result<Vec<DiscoveredModel>, LlmError> {
        let base = self.config.gateway_base_url.trim_end_matches('/');
        let endpoint = self
            .config
            .models_endpoint_override
            .as_deref()
            .unwrap_or("/v1/models");
        let url = format!("{}{}", base, endpoint);
        let headers = self.build_headers()?;

        let res = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        if !res.status().is_success() {
            return Err(LlmError::Provider {
                status: res.status().as_u16(),
                message: res.text().await.unwrap_or_default(),
            });
        }

        let body: OpenAiModelsResponse = res
            .json()
            .await
            .map_err(|e| LlmError::Serialization(e.to_string()))?;

        Ok(body.data)
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let base = self.config.gateway_base_url.trim_end_matches('/');
        let endpoint = self
            .config
            .chat_endpoint_override
            .as_deref()
            .unwrap_or("/v1/chat/completions");
        let url = format!("{}{}", base, endpoint);
        let headers = self.build_headers()?;

        let model = self.resolve_model_id(req.model.as_deref());
        let response_format = if req.response_format_json {
            Some(ResponseFormatWrapper {
                format_type: "json_object",
            })
        } else {
            None
        };

        let payload = OpenAiChatRequest {
            model: &model,
            messages: &req.messages,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            response_format,
        };

        let res = self
            .client
            .post(&url)
            .headers(headers)
            .json(&payload)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        if !res.status().is_success() {
            return Err(LlmError::Provider {
                status: res.status().as_u16(),
                message: res.text().await.unwrap_or_default(),
            });
        }

        let body: OpenAiChatResponse = res
            .json()
            .await
            .map_err(|e| LlmError::Serialization(e.to_string()))?;

        let choice = body
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Serialization("Response choices was empty".to_string()))?;

        let usage = body.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens.unwrap_or(0),
            completion_tokens: u.completion_tokens.unwrap_or(0),
            total_tokens: u.total_tokens.unwrap_or(0),
        });

        Ok(CompletionResponse {
            model: body.model.unwrap_or(model),
            content: choice.message.content.unwrap_or_default(),
            finish_reason: choice.finish_reason,
            usage,
        })
    }
}
