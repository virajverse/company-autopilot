use cos_llm::{
    ChatMessage, CompletionRequest, CredentialKind, GatewayAuthScheme, HttpInferenceGateway,
    LlmError, LlmGatewayConfig, LlmPort, ModelEntryConfig, ModelsConfig, UnconfiguredLlmGateway,
};
use std::collections::HashMap;

#[test]
fn test_gateway_config_ui_safe_masking() {
    let mut config = LlmGatewayConfig {
        gateway_api_key: "sk-ant-live-secret-key-123456789".to_string(),
        ..Default::default()
    };

    let safe = config.to_ui_safe();
    assert_ne!(safe.gateway_api_key, config.gateway_api_key);
    assert!(safe.gateway_api_key.starts_with("sk-"));
    assert!(safe.gateway_api_key.ends_with("789"));
    assert!(safe.gateway_api_key.contains("••••"));

    // Short key
    config.gateway_api_key = "short".to_string();
    let safe_short = config.to_ui_safe();
    assert_eq!(safe_short.gateway_api_key, "******");

    // Test fmt::Debug masking
    let debug_repr = format!("{:?}", config);
    assert!(!debug_repr.contains("sk-ant-live-secret-key-123456789"));
    assert!(debug_repr.contains("******"));
}

#[test]
fn test_gateway_config_serialization_matches_ui_fields() {
    let mut custom_headers = HashMap::new();
    custom_headers.insert("x-tenant-id".to_string(), "tenant-alpha".to_string());
    custom_headers.insert("x-guardrail-id".to_string(), "gr-sec-01".to_string());

    let config = LlmGatewayConfig {
        credential_kind: CredentialKind::StaticApiKey,
        gateway_base_url: "http://127.0.0.1:8008".to_string(),
        gateway_api_key: "sk-mock-key".to_string(),
        gateway_auth_scheme: GatewayAuthScheme::Bearer,
        artifact_preview_iframe_origin: Some("https://preview.internal.local".to_string()),
        custom_inference_headers: custom_headers,
        stream_idle_timeout_seconds: 300,
        gateway_managed_claude_code: false,
        models: ModelsConfig {
            model_discovery_enabled: true,
            prefer_1m_context: false,
            model_list: vec![
                ModelEntryConfig {
                    model_id: "claude-sonnet-4-5".to_string(),
                    display_name: Some("claude-sonnet-4-5".to_string()),
                    offer_1m_variant: true,
                    only_1m_option: false,
                    is_default: true,
                },
                ModelEntryConfig {
                    model_id: "gpt-4o".to_string(),
                    display_name: Some("GPT-4o".to_string()),
                    offer_1m_variant: false,
                    only_1m_option: false,
                    is_default: false,
                },
            ],
        },
        chat_endpoint_override: None,
        models_endpoint_override: None,
    };

    let json_str = serde_json::to_string_pretty(&config).expect("Serialize to JSON");
    assert!(json_str.contains("claude-sonnet-4-5"));
    assert!(json_str.contains("stream_idle_timeout_seconds"));
    assert!(json_str.contains("offer_1m_variant"));

    let deserialized: LlmGatewayConfig =
        serde_json::from_str(&json_str).expect("Deserialize from JSON");
    assert_eq!(deserialized.models.model_list.len(), 2);
    assert!(deserialized.models.model_list[0].offer_1m_variant);
    assert_eq!(
        deserialized
            .custom_inference_headers
            .get("x-tenant-id")
            .unwrap(),
        "tenant-alpha"
    );
}

#[tokio::test]
async fn test_unconfigured_llm_gateway_returns_real_errors() {
    let unconfigured = UnconfiguredLlmGateway::new("Missing API key");

    // 1. Connection test explicitly reports unconfigured status
    let conn = unconfigured.test_connection().await.unwrap();
    assert!(!conn.success);
    assert!(conn.message.contains("Missing API key"));

    // 2. Discover models cleanly returns real configuration error
    let models_res = unconfigured.discover_models().await;
    assert!(matches!(models_res, Err(LlmError::Configuration(_))));

    // 3. Completion returns explicit configuration error rather than fake synthetic pass
    let req = CompletionRequest::new(vec![ChatMessage::user("Verify operational parameters")]);
    let comp_res = unconfigured.complete(req).await;
    assert!(matches!(comp_res, Err(LlmError::Configuration(_))));
}

#[test]
fn test_http_inference_gateway_instantiation() {
    let config = LlmGatewayConfig::default();
    let gw = HttpInferenceGateway::new(config);
    assert!(gw.is_ok());
}
