//! HTTP Request Forwarder
//!
//! Forwards requests to upstream API and streams responses back.

use crate::config::ConfigState;
use crate::proxy::error::ProxyError;
use crate::proxy::types::{UpstreamInfo, UpstreamProtocol};
use serde_json::Value;
use std::collections::HashMap;

/// Resolve upstream info from config state + model
pub async fn resolve_upstream(
    config: &ConfigState,
    model: &str,
) -> Result<UpstreamInfo, ProxyError> {
    let provider = config
        .find_provider(model)
        .await
        .ok_or_else(|| ProxyError::ConfigError(format!("没有为模型 '{model}' 配置上游")))?;

    let upstream_name = provider.id.clone();
    let meta = provider.meta.as_ref();
    let protocol = match meta.and_then(|m| m.api_format.as_deref()) {
        Some("openai_chat") | Some("openai") => UpstreamProtocol::OpenAI,
        Some("openai_responses") => UpstreamProtocol::OpenAIResponses,
        Some("gemini_native") => UpstreamProtocol::Gemini,
        _ => UpstreamProtocol::Anthropic,
    };

    let settings = &provider.settings_config;
    let (base_url, api_key) = resolve_credentials(settings, protocol);

    // model_mappings are resolved at use-time via ConfigState.get_model_mapping()

    Ok(UpstreamInfo {
        name: upstream_name.clone(),
        base_url,
        api_key,
        protocol,
        model_mappings: HashMap::new(),
        custom_headers: HashMap::new(), // populated per-request from config
    })
}

fn resolve_credentials(settings: &Value, protocol: UpstreamProtocol) -> (String, String) {
    match protocol {
        UpstreamProtocol::Anthropic => {
            let env = settings.get("env");
            let url = env
                .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
                .and_then(|v| v.as_str())
                .unwrap_or("https://api.anthropic.com");
            // ANTHROPIC_AUTH_TOKEN 优先，ANTHROPIC_API_KEY 回退
            let key = env
                .and_then(|e| e.get("ANTHROPIC_AUTH_TOKEN").or_else(|| e.get("ANTHROPIC_API_KEY")))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            (url.to_string(), key.to_string())
        }
        UpstreamProtocol::OpenAI | UpstreamProtocol::OpenAIResponses => {
            let url = settings
                .get("baseUrl")
                .and_then(|v| v.as_str())
                .unwrap_or("https://api.openai.com");
            let key = settings
                .get("apiKey")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            (url.to_string(), key.to_string())
        }
        UpstreamProtocol::Gemini => {
            let env = settings.get("env");
            let url = env
                .and_then(|e| e.get("GEMINI_BASE_URL"))
                .and_then(|v| v.as_str())
                .unwrap_or("https://generativelanguage.googleapis.com");
            let key = env
                .and_then(|e| e.get("GEMINI_API_KEY"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            (url.to_string(), key.to_string())
        }
    }
}

/// Build auth headers for upstream request
pub fn build_auth_headers(upstream: &UpstreamInfo) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    match upstream.protocol {
        UpstreamProtocol::Anthropic => {
            // 自动选择认证方式：sk-ant 开头的 API key 用 x-api-key，否则用 Bearer token
            if upstream.api_key.starts_with("sk-ant-") {
                headers.push(("x-api-key".to_string(), upstream.api_key.clone()));
            } else {
                headers.push((
                    "Authorization".to_string(),
                    format!("Bearer {}", upstream.api_key),
                ));
            }
            headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
        }
        UpstreamProtocol::OpenAI | UpstreamProtocol::OpenAIResponses => {
            headers.push((
                "Authorization".to_string(),
                format!("Bearer {}", upstream.api_key),
            ));
        }
        UpstreamProtocol::Gemini => {} // API key goes in query string
    }
    // Append custom headers
    for (k, v) in &upstream.custom_headers {
        headers.push((k.clone(), v.clone()));
    }
    headers
}

/// Build the upstream URL for a given endpoint
pub fn build_upstream_url(upstream: &UpstreamInfo, path: &str) -> String {
    let base = upstream.base_url.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{base}/{path}")
}

/// Validate a model name to prevent URL injection
pub fn validate_model_name(model: &str) -> Result<(), ProxyError> {
    if model.is_empty() {
        return Err(ProxyError::InvalidRequest("model 名不能为空".into()));
    }
    if model.contains("..") || model.contains('/') || model.contains('\\') || model.contains('#') || model.contains('?') {
        return Err(ProxyError::InvalidRequest(format!("非法的 model 名: {model}")));
    }
    Ok(())
}
