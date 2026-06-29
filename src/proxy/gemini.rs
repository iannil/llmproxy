// Anthropic ↔ Gemini conversion paths

use serde_json::{json, Value};
use crate::proxy::error::ProxyError;

/// Convert Anthropic Messages request to Gemini format
pub fn anthropic_to_gemini(body: &Value) -> Result<Value, ProxyError> {
    let _model = body["model"]
        .as_str()
        .ok_or_else(|| ProxyError::InvalidRequest("缺少 model 字段".into()))?;

    let mut contents = Vec::new();
    let mut system_text = String::new();

    // Extract system prompt
    if let Some(system) = body.get("system").and_then(|s| s.as_str()) {
        system_text = system.to_string();
    }

    // Convert messages
    if let Some(messages) = body["messages"].as_array() {
        for msg in messages {
            let role = match msg["role"].as_str() {
                Some("assistant") => "model",
                _ => "user",
            };

            let mut parts = Vec::new();
            let content = &msg["content"];

            if content.is_array() {
                for block in content.as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
                    match block["type"].as_str() {
                        Some("text") => {
                            if let Some(text) = block["text"].as_str() {
                                parts.push(json!({"text": text}));
                            }
                        }
                        Some("tool_use") => {
                            // Tool use is from assistant, pass through as functionCall
                            if let (Some(name), Some(_id)) = (
                                block["name"].as_str(),
                                block["id"].as_str(),
                            ) {
                                parts.push(json!({
                                    "functionCall": {
                                        "name": name,
                                        "args": block["input"]
                                    }
                                }));
                            }
                        }
                        Some("tool_result") => {
                            // Tool result → functionResponse
                            if let Some(id) = block["tool_use_id"].as_str() {
                                let result_content = block["content"].as_str().unwrap_or("");
                                parts.push(json!({
                                    "functionResponse": {
                                        "name": id,
                                        "response": {"content": result_content}
                                    }
                                }));
                            }
                        }
                        _ => {}
                    }
                }
            } else if let Some(text) = content.as_str() {
                parts.push(json!({"text": text}));
            }

            if !parts.is_empty() {
                contents.push(json!({
                    "role": role,
                    "parts": parts
                }));
            }
        }
    }

    let mut result = json!({
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": body.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(4096),
        }
    });

    // System instruction
    if !system_text.is_empty() {
        result["systemInstruction"] = json!({
            "parts": [{"text": system_text}]
        });
    }

    // Temperature
    if let Some(temp) = body.get("temperature").and_then(|v| v.as_f64()) {
        result["generationConfig"]["temperature"] = json!(temp);
    }

    // Tools
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let gemini_tools: Vec<Value> = tools.iter().map(|t| {
            json!({
                "functionDeclarations": [{
                    "name": t["name"],
                    "description": t["description"],
                    "parameters": t["input_schema"]
                }]
            })
        }).collect();
        if !gemini_tools.is_empty() {
            result["tools"] = json!(gemini_tools);
        }
    }

    Ok(result)
}

/// Convert Gemini response to Anthropic Messages format
pub fn gemini_to_anthropic(body: &Value, model: &str) -> Result<Value, ProxyError> {
    let candidates: &[serde_json::Value] = body["candidates"].as_array().map(|v| v.as_slice()).unwrap_or(&[]);
    let candidate = candidates.first();

    let mut content_blocks = Vec::new();

    if let Some(c) = candidate {
        if let Some(content) = c.get("content") {
            if let Some(parts) = content["parts"].as_array() {
                for part in parts {
                    if let Some(text) = part["text"].as_str() {
                        content_blocks.push(json!({"type": "text", "text": text}));
                    }
                    if let Some(fc) = part.get("functionCall") {
                        if let (Some(name), Some(input)) = (
                            fc["name"].as_str(),
                            fc.get("args"),
                        ) {
                            content_blocks.push(json!({
                                "type": "tool_use",
                                "id": format!("toolu_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
                                "name": name,
                                "input": input
                            }));
                        }
                    }
                }
            }
        }
    }

    let stop_reason = candidate
        .and_then(|c| c["finishReason"].as_str())
        .map(|r| match r {
            "STOP" => "end_turn",
            "MAX_TOKENS" => "max_tokens",
            "SAFETY" => "error",
            "RECITATION" => "error",
            "OTHER" => "error",
            _ => "end_turn",
        });

    // Usage
    let usage = body.get("usageMetadata").map(|u| json!({
        "input_tokens": u.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0),
        "output_tokens": u.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0),
    }));

    let msg_id = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let result = json!({
        "id": msg_id,
        "type": "message",
        "role": "assistant",
        "content": content_blocks,
        "model": model,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": usage.unwrap_or(json!({
            "input_tokens": 0,
            "output_tokens": 0
        }))
    });

    Ok(result)
}
