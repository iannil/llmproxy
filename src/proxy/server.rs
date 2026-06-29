//! HTTP Proxy Server — Lite version
//!
//! Axum-based proxy that accepts multi-protocol requests and forwards them
//! to the appropriate upstream with protocol conversion.

use crate::config::ConfigState;
use crate::proxy::{
    convert, gemini,
    forwarder::{build_auth_headers, build_upstream_url, resolve_upstream, validate_model_name},
    streaming::{AnthropicToOpenAiStream, OpenAiToAnthropicStream},
    types::{UpstreamInfo, UpstreamProtocol},
};

use axum::{
    body::Body,
    extract::State,
    http::{HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, post},
    Json, Router,
};
use futures::StreamExt;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

/// Reusable HTTP client (created once to share connection pool)

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ConfigState>,
    pub client: reqwest::Client,
}

/// Start the proxy server
pub async fn start(config: Arc<ConfigState>) {
    let listen = config.listen_addr().await;
    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(config.upstream_timeout_secs().await))
        .pool_max_idle_per_host(32)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("无法创建 HTTP 客户端: {e}");
            std::process::exit(1);
        }
    };
    let state = AppState { config, client };

    let app = Router::new()
        .route("/v1/messages", post(handle_anthropic_messages))
        .route("/v1/chat/completions", post(handle_openai_chat))
        .route("/v1/responses", post(handle_openai_responses))
        .route("/v1/models", any(handle_models))
        .route("/v1beta/models", any(handle_models))
        .route("/v1beta/*path", any(handle_gemini))
        .route("/health", any(handle_health))
        .route("/*path", any(handle_fallback))
        .layer(axum::extract::DefaultBodyLimit::max(usize::MAX))
        .with_state(state);

    let addr: std::net::SocketAddr = match listen.parse() {
        Ok(a) => a,
        Err(_) => {
            log::error!("无效的监听地址: {listen}");
            std::process::exit(1);
        }
    };

    log::info!("Proxy server listening on {addr}");

    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            if let Err(e) = axum::serve(listener, app).await {
                log::error!("服务器运行错误: {e}");
            }
        }
        Err(e) => {
            log::error!("无法绑定到 {addr}: {e}");
            std::process::exit(1);
        }
    }
}

// ─── Route Handlers ─────────────────────────────────────────────

async fn handle_anthropic_messages(
    State(state): State<AppState>,
    body: Json<Value>,
) -> Response {
    let body = body.0;
    let model = body["model"].as_str().unwrap_or("unknown");

    // Validate model name
    if let Err(e) = validate_model_name(model) {
        log::warn!("拦截非法 model 名: {model}");
        return error_response(StatusCode::BAD_REQUEST, &e.to_string());
    }

    let upstream = match resolve_upstream(&state.config, model).await {
        Ok(u) => u,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };

    let (url, send_body) = match upstream.protocol {
        UpstreamProtocol::Anthropic => {
            // Pass-through
            (build_upstream_url(&upstream, "/v1/messages"), body)
        }
        UpstreamProtocol::OpenAI | UpstreamProtocol::OpenAIResponses => {
            // Anthropic → OpenAI Chat
            let converted = match convert::anthropic_to_openai_chat(&body) {
                Ok(c) => c,
                Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
            };
            let url = build_upstream_url(&upstream, "/v1/chat/completions");
            return forward_json(&state, &upstream, &url, &converted, SseMode::OpenAiToAnthropic).await; // OpenAI SSE → Anthropic SSE
        }
        UpstreamProtocol::Gemini => {
            let converted = match gemini::anthropic_to_gemini(&body) {
                Ok(c) => c,
                Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
            };
            let url = build_upstream_url(&upstream, &format!("/v1beta/models/{model}:streamGenerateContent?key={}", upstream.api_key));
            return forward_json(&state, &upstream, &url, &converted, SseMode::Passthrough).await;
        }
    };

    forward_json(&state, &upstream, &url, &send_body, SseMode::Passthrough).await
}

async fn handle_openai_chat(
    State(state): State<AppState>,
    body: Json<Value>,
) -> Response {
    let body = body.0;
    let model = body["model"].as_str().unwrap_or("unknown");

    if let Err(e) = validate_model_name(model) {
        log::warn!("拦截非法 model 名: {model}");
        return error_response(StatusCode::BAD_REQUEST, &e.to_string());
    }

    let upstream = match resolve_upstream(&state.config, model).await {
        Ok(u) => u,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };

    let (url, send_body) = match upstream.protocol {
        UpstreamProtocol::OpenAI => {
            (build_upstream_url(&upstream, "/v1/chat/completions"), body)
        }
        UpstreamProtocol::OpenAIResponses => {
            // Chat → Responses conversion
            let converted = match convert::chat_to_responses(&body) {
                Ok(c) => c,
                Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
            };
            let url = build_upstream_url(&upstream, "/v1/responses");
            return forward_json(&state, &upstream, &url, &converted, SseMode::Passthrough).await;
        }
        UpstreamProtocol::Anthropic => {
            // OpenAI Chat → Anthropic Messages
            let converted = match convert::openai_chat_to_anthropic(&body) {
                Ok(c) => c,
                Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
            };
            let url = build_upstream_url(&upstream, "/v1/messages");
            return forward_json(&state, &upstream, &url, &converted, SseMode::AnthropicToOpenAi).await;
        }
        UpstreamProtocol::Gemini => {
            let converted = match gemini::anthropic_to_gemini(&body) {
                Ok(c) => c,
                Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
            };
            let url = build_upstream_url(&upstream, &format!("/v1beta/models/{model}:streamGenerateContent?key={}", upstream.api_key));
            return forward_json(&state, &upstream, &url, &converted, SseMode::Passthrough).await;
        }
    };

    forward_json(&state, &upstream, &url, &send_body, SseMode::Passthrough).await
}

async fn handle_openai_responses(
    State(state): State<AppState>,
    body: Json<Value>,
) -> Response {
    let body = body.0;
    let model = body["model"].as_str().unwrap_or("unknown");

    if let Err(e) = validate_model_name(model) {
        log::warn!("拦截非法 model 名: {model}");
        return error_response(StatusCode::BAD_REQUEST, &e.to_string());
    }

    let upstream = match resolve_upstream(&state.config, model).await {
        Ok(u) => u,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };

    let (url, send_body) = match upstream.protocol {
        UpstreamProtocol::OpenAIResponses => {
            (build_upstream_url(&upstream, "/v1/responses"), body)
        }
        UpstreamProtocol::OpenAI => {
            // Responses → Chat conversion
            let converted = match convert::responses_to_chat(&body) {
                Ok(c) => c,
                Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
            };
            let url = build_upstream_url(&upstream, "/v1/chat/completions");
            return forward_json(&state, &upstream, &url, &converted, SseMode::Passthrough).await;
        }
        UpstreamProtocol::Anthropic => {
            // Responses → Anthropic
            let converted = match convert::openai_chat_to_anthropic(&body) {
                Ok(c) => c,
                Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
            };
            let url = build_upstream_url(&upstream, "/v1/messages");
            return forward_json(&state, &upstream, &url, &converted, SseMode::AnthropicToOpenAi).await;
        }
        UpstreamProtocol::Gemini => {
            let converted = match gemini::anthropic_to_gemini(&body) {
                Ok(c) => c,
                Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
            };
            let url = build_upstream_url(&upstream, &format!("/v1beta/models/{model}:streamGenerateContent?key={}", upstream.api_key));
            return forward_json(&state, &upstream, &url, &converted, SseMode::Passthrough).await;
        }
    };

    forward_json(&state, &upstream, &url, &send_body, SseMode::Passthrough).await
}

async fn handle_models(
    State(state): State<AppState>,
) -> Json<Value> {
    let providers = state.config.all_providers().await;
    let models: Vec<Value> = providers
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "name": p.name,
                "object": "model",
                "created": 0,
                "owned_by": "llmproxy"
            })
        })
        .collect();

    Json(json!({
        "object": "list",
        "data": models
    }))
}

async fn handle_gemini(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
    body: Json<Value>,
) -> Response {
    // Validate path — only allow known Gemini patterns
    if !path.starts_with("models/") && !path.starts_with("tunedModels/") {
        log::warn!("拦截非法的 Gemini 路径: /v1beta/{path}");
        return error_response(StatusCode::BAD_REQUEST, &format!("不支持的端点: /v1beta/{path}"));
    }

    let Json(body_value) = body;
    let model = body_value
        .get("model")
        .and_then(|m| m.as_str())
        .or_else(|| path.split('/').find(|part| part.starts_with("gemini")))
        .unwrap_or("gemini-2.5-flash");

    let upstream = match resolve_upstream(&state.config, model).await {
        Ok(u) => u,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };

    match upstream.protocol {
        UpstreamProtocol::Anthropic | UpstreamProtocol::OpenAI | UpstreamProtocol::OpenAIResponses => {
            // Extract text from Gemini format and create a simple message
            let text = body_value.get("contents")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|c| c["parts"].as_array())
                .and_then(|parts| parts.first())
                .and_then(|p| p["text"].as_str())
                .unwrap_or("");

            let anthropic_body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": text}],
                "max_tokens": 4096
            });

            let url = build_upstream_url(&upstream, "/v1/messages");
            return forward_json(&state, &upstream, &url, &anthropic_body, SseMode::Passthrough).await;
        }
        UpstreamProtocol::Gemini => {
            let url = build_upstream_url(&upstream, &format!("/v1beta/{path}?key={}", upstream.api_key));
            forward_json(&state, &upstream, &url, &body_value, SseMode::Passthrough).await
        }
    }
}

async fn handle_health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "service": "llmproxy"
    }))
}

async fn handle_fallback(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    error_response(StatusCode::NOT_FOUND, &format!("未知端点: /{path}"))
}

// ─── Core Forwarding ────────────────────────────────────────────

/// SSE conversion mode when forwarding
enum SseMode {
    Passthrough,
    OpenAiToAnthropic,
    AnthropicToOpenAi,
}

/// Forward a JSON body to upstream and return the response
async fn forward_json(
    state: &AppState,
    upstream: &UpstreamInfo,
    url: &str,
    body: &Value,
    sse_mode: SseMode,
) -> Response {
    let mut req_builder = state.client.post(url).json(body);

    // Apply model mapping: use ConfigState lookup instead of UpstreamInfo HashMap
    let request_model = body.get("model").and_then(|m| m.as_str()).unwrap_or("");
    if !request_model.is_empty() {
        if let Some(upstream_model) = state.config.get_model_mapping(&upstream.name, request_model).await {
            if let Some(body_map) = body.clone().as_object().cloned() {
                let mut map = body_map.clone();
                map.insert("model".to_string(), json!(upstream_model));
                // Re-build request with mapped model
                req_builder = state.client.post(url).json(&Value::Object(map));
            }
        }
    }

    for (name, value) in build_auth_headers(upstream) {
        if let (Ok(h_name), Ok(h_val)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            req_builder = req_builder.header(h_name, h_val);
        }
    }

    match req_builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            let headers = resp.headers().clone();

            if !status.is_success() {
                // Read error body and return it to client
                let err_body = resp.bytes().await.unwrap_or_default();
                let err_text = String::from_utf8_lossy(&err_body);
                return error_response(status, &err_text);
            }

            if is_sse_response(&headers, body) {
                match sse_mode {
                    SseMode::OpenAiToAnthropic => {
                        let model = body["model"].as_str().unwrap_or("unknown").to_string();
                        let converter = std::sync::Mutex::new(OpenAiToAnthropicStream::new(&model));
                        let stream = resp.bytes_stream().map(move |chunk| {
                            let chunk_data = match chunk {
                                Ok(b) => String::from_utf8_lossy(&b).to_string(),
                                Err(e) => format!("data: [error: {e}]\n\n"),
                            };
                            let converted = converter.lock().unwrap().push(&chunk_data).unwrap_or_default();
                            Ok::<_, Infallible>(axum::body::Bytes::from(converted))
                        });
                        let body = Body::from_stream(stream);
                        Response::builder()
                            .status(StatusCode::OK)
                            .header("content-type", "text/event-stream")
                            .header("cache-control", "no-cache")
                            .header("connection", "keep-alive")
                            .body(body)
                            .unwrap()
                    }
                    SseMode::AnthropicToOpenAi => {
                        let model = body["model"].as_str().unwrap_or("unknown").to_string();
                        let converter = std::sync::Mutex::new(AnthropicToOpenAiStream::new(&model));
                        let stream = resp.bytes_stream().map(move |chunk| {
                            let chunk_data = match chunk {
                                Ok(b) => String::from_utf8_lossy(&b).to_string(),
                                Err(e) => format!("data: [error: {e}]\n\n"),
                            };
                            let converted = converter.lock().unwrap().push(&chunk_data).unwrap_or_default();
                            Ok::<_, Infallible>(axum::body::Bytes::from(converted))
                        });
                        let body = Body::from_stream(stream);
                        Response::builder()
                            .status(StatusCode::OK)
                            .header("content-type", "text/event-stream")
                            .header("cache-control", "no-cache")
                            .header("connection", "keep-alive")
                            .body(body)
                            .unwrap()
                    }
                    SseMode::Passthrough => {
                    // Passthrough SSE
                    let stream = resp.bytes_stream().map(|chunk| {
                        let data = match chunk {
                            Ok(b) => String::from_utf8_lossy(&b).to_string(),
                            Err(e) => format!("data: [error: {e}]\n\n"),
                        };
                        Ok::<_, Infallible>(axum::body::Bytes::from(data))
                    });
                    let body = Body::from_stream(stream);
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/event-stream")
                        .header("cache-control", "no-cache")
                        .header("connection", "keep-alive")
                        .body(body)
                        .unwrap()
                }
                }
            } else {
                // Non-streaming response
                let resp_bytes = resp.bytes().await.unwrap_or_default();
                let resp_body: Value =
                    serde_json::from_slice(&resp_bytes).unwrap_or(json!({}));
                (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY), Json(resp_body))
                    .into_response()
            }
        }
        Err(e) => {
            let status = if e.is_timeout() {
                StatusCode::GATEWAY_TIMEOUT
            } else if e.is_connect() {
                StatusCode::BAD_GATEWAY
            } else {
                StatusCode::BAD_GATEWAY
            };
            error_response(status, &format!("上游错误: {e}"))
        }
    }
}

/// Determine if upstream response is SSE based on Content-Type + request stream flag
fn is_sse_response(headers: &axum::http::HeaderMap, body: &Value) -> bool {
    // If client requested stream=false, always return non-streaming
    if body.get("stream").and_then(|s| s.as_bool()) == Some(false) {
        return false;
    }
    // Otherwise check Content-Type
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/event-stream") || ct.contains("text/plain"))
        .unwrap_or(false)
}

fn error_response(status: StatusCode, message: &str) -> Response {
    log::warn!("HTTP {}: {}", status.as_u16(), message);
    let body = json!({
        "error": {
            "type": "error",
            "message": message
        }
    });
    (status, Json(body)).into_response()
}
