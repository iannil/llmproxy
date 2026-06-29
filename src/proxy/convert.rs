//! Protocol conversion between Anthropic Messages ↔ OpenAI Chat Completions
//!
//! Core conversion paths:
//!   Anthropic Messages → OpenAI Chat (Claude Code using OpenAI backend)
//!   OpenAI Chat → Anthropic Messages (Codex using Anthropic backend)

use serde_json::{json, Value};
use crate::proxy::error::ProxyError;

/// Convert OpenAI Chat Completions request body to Anthropic Messages format
pub fn openai_chat_to_anthropic(body: &Value) -> Result<Value, ProxyError> {
    let model = body["model"]
        .as_str()
        .ok_or_else(|| ProxyError::InvalidRequest("缺少 model 字段".into()))?;

    let oai_messages = body["messages"]
        .as_array()
        .ok_or_else(|| ProxyError::InvalidRequest("缺少 messages 字段".into()))?;

    let mut anthropic_messages = Vec::new();
    let mut system_text = String::new();

    for msg in oai_messages {
        let role = msg["role"].as_str().unwrap_or("user");
        let content = msg["content"].as_str().unwrap_or("");

        match role {
            "system" => {
                system_text.push_str(content);
                system_text.push('\n');
            }
            "assistant" => {
                // Check for tool calls
                let tool_calls = msg.get("tool_calls").and_then(|t| t.as_array());
                if let Some(tcs) = tool_calls {
                    // Message with tool calls
                    let mut content_blocks = Vec::new();
                    if !content.is_empty() {
                        content_blocks.push(json!({"type": "text", "text": content}));
                    }
                    for tc in tcs {
                        if let (Some(name), Some(id)) = (
                            tc["function"]["name"].as_str(),
                            tc.get("id").and_then(|i| i.as_str()),
                        ) {
                            let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                            let args: Value =
                                serde_json::from_str(args_str).unwrap_or(json!({}));
                            content_blocks.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": args
                            }));
                        }
                    }
                    anthropic_messages.push(json!({
                        "role": "assistant",
                        "content": content_blocks
                    }));
                } else {
                    anthropic_messages.push(json!({
                        "role": "assistant",
                        "content": content
                    }));
                }
            }
            "tool" => {
                // OpenAI tool_result → Anthropic tool_result
                let tool_call_id = msg.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or("");
                anthropic_messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": content
                    }]
                }));
            }
            _ => {
                anthropic_messages.push(json!({
                    "role": "user",
                    "content": content
                }));
            }
        }
    }

    let mut result = json!({
        "model": model,
        "messages": anthropic_messages,
        "max_tokens": body.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(4096),
    });

    if !system_text.is_empty() {
        result["system"] = json!(system_text.trim());
    }

    // Convert tools
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let anthropic_tools: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                let func = t.get("function")?;
                let name = func.get("name")?.as_str()?;
                let desc = func.get("description").and_then(|d| d.as_str()).unwrap_or("");
                let input_schema = func.get("parameters");
                Some(json!({
                    "name": name,
                    "description": desc,
                    "input_schema": input_schema.unwrap_or(&json!({"type": "object"})).clone()
                }))
            })
            .collect();
        if !anthropic_tools.is_empty() {
            result["tools"] = json!(anthropic_tools);
        }
    }

    Ok(result)
}

/// Convert Anthropic Messages request body to OpenAI Chat Completions format
pub fn anthropic_to_openai_chat(body: &Value) -> Result<Value, ProxyError> {
    let model = body["model"]
        .as_str()
        .ok_or_else(|| ProxyError::InvalidRequest("缺少 model 字段".into()))?;

    // Extract system prompt
    let system_text = body.get("system")
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let mut messages: Vec<Value> = Vec::new();

    // Prepend system message if present
    if !system_text.is_empty() {
        messages.push(json!({"role": "system", "content": system_text}));
    }

    // Convert Anthropic messages
    if let Some(anthropic_messages) = body["messages"].as_array() {
        for msg in anthropic_messages {
            let role = msg["role"].as_str().unwrap_or("user");
            let content = &msg["content"];

            match role {
                "assistant" => {
                    let mut text = String::new();
                    let mut tool_calls = Vec::new();

                    if content.is_array() {
                        for block in content.as_array().map_or::<&[Value], _>(&[], |v| v.as_slice()) {
                            match block["type"].as_str() {
                                Some("text") => {
                                    if let Some(t) = block["text"].as_str() {
                                        text.push_str(t);
                                    }
                                }
                                Some("tool_use") => {
                                    let name = block["name"].as_str().unwrap_or("");
                                    let id = block["id"].as_str().unwrap_or("");
                                    let input = &block["input"];
                                    tool_calls.push(json!({
                                        "id": id,
                                        "type": "function",
                                        "function": {
                                            "name": name,
                                            "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string())
                                        }
                                    }));
                                }
                                _ => {}
                            }
                        }
                    } else if let Some(t) = content.as_str() {
                        text = t.to_string();
                    }

                    let mut oai_msg = json!({
                        "role": "assistant",
                        "content": text
                    });

                    if !tool_calls.is_empty() {
                        oai_msg["tool_calls"] = json!(tool_calls);
                    }

                    messages.push(oai_msg);
                }
                "user" => {
                    let mut parts = Vec::new();

                    if content.is_array() {
                        for block in content.as_array().map_or::<&[Value], _>(&[], |v| v.as_slice()) {
                            match block["type"].as_str() {
                                Some("text") => {
                                    if let Some(t) = block["text"].as_str() {
                                        parts.push(json!({"type": "text", "text": t}));
                                    }
                                }
                                Some("tool_result") => {
                                    let id = block["tool_use_id"].as_str().unwrap_or("");
                                    let result_text = match &block["content"] {
                                        Value::String(s) => s.clone(),
                                        Value::Array(arr) => {
                                            arr.iter()
                                                .find(|b| b["type"] == "text")
                                                .and_then(|b| b["text"].as_str())
                                                .unwrap_or("")
                                                .to_string()
                                        }
                                        _ => "".to_string(),
                                    };
                                    parts.push(json!({
                                        "type": "tool_result",
                                        "tool_use_id": id,
                                        "content": result_text
                                    }));
                                }
                                _ => {}
                            }
                        }
                    } else if let Some(t) = content.as_str() {
                        parts.push(json!({"type": "text", "text": t}));
                    }

                    // OpenAI Chat expects simple string content for user
                    let user_text: String = parts.iter()
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");

                    messages.push(json!({
                        "role": "user",
                        "content": if user_text.is_empty() {
                            serde_json::Value::Null
                        } else {
                            json!(user_text)
                        }
                    }));
                }
                _ => {
                    messages.push(json!({
                        "role": "user",
                        "content": content.as_str().unwrap_or("")
                    }));
                }
            }
        }
    }

    let mut result = json!({
        "model": model,
        "messages": messages,
        "stream": body.get("stream").and_then(|s| s.as_bool()).unwrap_or(true),
    });

    // Pass through max_tokens (default 4096 if missing)
    result["max_tokens"] = json!(body.get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(4096));

    // Pass through temperature
    if let Some(temp) = body.get("temperature").and_then(|v| v.as_f64()) {
        result["temperature"] = json!(temp);
    }

    // Convert tools (Anthropic → OpenAI format)
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let openai_tools: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                let name = t.get("name")?.as_str()?;
                let desc = t.get("description").and_then(|d| d.as_str()).unwrap_or("");
                let input_schema = t.get("input_schema");
                Some(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": desc,
                        "parameters": input_schema.cloned().unwrap_or(json!({"type": "object"}))
                    }
                }))
            })
            .collect();
        if !openai_tools.is_empty() {
            result["tools"] = json!(openai_tools);
        }
    }

    Ok(result)
}

// ─── OpenAI Chat ↔ OpenAI Responses conversion ─────────────

/// Convert OpenAI Chat Completions request to OpenAI Responses format
pub fn chat_to_responses(body: &Value) -> Result<Value, ProxyError> {
    let model = body["model"]
        .as_str()
        .ok_or_else(|| ProxyError::InvalidRequest("缺少 model 字段".into()))?;

    let messages = body["messages"].as_array();
    let input: Value = if let Some(msgs) = messages {
        if msgs.len() == 1 && msgs[0]["role"] == "user" {
            json!(msgs[0]["content"].as_str().unwrap_or(""))
        } else {
            json!(msgs)
        }
    } else {
        json!("")
    };

    let mut result = json!({
        "model": model,
        "input": input,
        "stream": body.get("stream").and_then(|s| s.as_bool()).unwrap_or(true),
    });

    if let Some(maxt) = body.get("max_tokens").and_then(|v| v.as_u64()) {
        result["max_output_tokens"] = json!(maxt);
    }

    if let Some(temp) = body.get("temperature").and_then(|v| v.as_f64()) {
        result["temperature"] = json!(temp);
    }

    if let Some(tools) = body.get("tools") {
        result["tools"] = tools.clone();
    }

    Ok(result)
}

/// Convert OpenAI Responses request to OpenAI Chat Completions format
pub fn responses_to_chat(body: &Value) -> Result<Value, ProxyError> {
    let model = body["model"]
        .as_str()
        .ok_or_else(|| ProxyError::InvalidRequest("缺少 model 字段".into()))?;

    let messages: Vec<Value> = match body.get("input") {
        Some(Value::String(text)) => {
            vec![json!({"role": "user", "content": text})]
        }
        Some(Value::Array(arr)) => arr.clone(),
        _ => vec![json!({"role": "user", "content": ""})],
    };

    let mut result = json!({
        "model": model,
        "messages": messages,
        "stream": body.get("stream").and_then(|s| s.as_bool()).unwrap_or(true),
    });

    if let Some(maxt) = body.get("max_output_tokens").and_then(|v| v.as_u64()) {
        result["max_tokens"] = json!(maxt);
    } else {
        result["max_tokens"] = json!(4096);
    }

    if let Some(temp) = body.get("temperature").and_then(|v| v.as_f64()) {
        result["temperature"] = json!(temp);
    }

    if let Some(tools) = body.get("tools") {
        result["tools"] = tools.clone();
    }

    Ok(result)
}

/// Convert OpenAI Chat Completions response chunk to Anthropic SSE format
pub fn openai_chunk_to_anthropic_sse(chunk: &Value, _model: &str) -> Option<String> {
    let choices = chunk["choices"].as_array()?;
    let delta = choices.first()?["delta"].as_object()?;

    let content = delta.get("content").and_then(|c| c.as_str()).unwrap_or("");
    let finish = choices.first()?["finish_reason"].as_str();

    if let Some(finish_reason) = finish {
        if finish_reason == "stop" || finish_reason == "end_turn" {
            return Some(format!(
                r#"event: message_stop
data: {{"type":"message_stop"}}

"#
            ));
        }
        if finish_reason == "tool_calls" {
            // Tool calls will be in the delta
        }
    }

    // Build Anthropic content block
    let is_empty = content.is_empty() && !delta.contains_key("tool_calls");
    if is_empty && finish.is_none() {
        return None; // Skip empty chunks
    }

    if !content.is_empty() {
        let escaped = content
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        return Some(format!(
            r#"event: content_block_delta
data: {{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{escaped}"}}}}

"#
        ));
    }

    // Tool calls
    if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tool_calls {
            if let (Some(name), Some(_id)) = (
                tc["function"]["name"].as_str(),
                tc.get("id").and_then(|id| id.as_str()),
            ) {
                let args = tc["function"]["arguments"].as_str().unwrap_or("{}");
                let escaped_args = args.replace('\\', "\\\\").replace('"', "\\\"");
                return Some(format!(
                    r#"event: content_block_start
data: {{"type":"content_block_start","index":0,"content_block":{{"type":"tool_use","id":"{id}","name":"{name}","input":{{}}}}}}

event: content_block_delta
data: {{"type":"content_block_delta","index":0,"delta":{{"type":"input_json_delta","partial_json":"{escaped_args}"}}}}

"#,
                    id = tc["id"].as_str().unwrap_or("toolu_000000"),
                    name = name,
                ));
            }
        }
    }

    None
}

/// Convert OpenAI Chat Completions response start to Anthropic message_start
pub fn openai_response_to_anthropic_start(response: &Value, model: &str) -> String {
    // Extract the response content
    let content = response
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c["message"].as_object());

    let mut anthropic_content = Vec::new();
    let mut stop_reason: Option<&str> = None;
    let mut usage: Option<Value> = None;

    if let Some(msg) = content {
        let text = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if !text.is_empty() {
            anthropic_content.push(json!({"type": "text", "text": text}));
        }

        stop_reason = response
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c["finish_reason"].as_str())
            .map(|r| match r {
                "stop" => "end_turn",
                "tool_calls" => "tool_use",
                "length" => "max_tokens",
                _ => r,
            });

        // Tool calls
        if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                if let (Some(name), Some(id)) = (
                    tc["function"]["name"].as_str(),
                    tc.get("id").and_then(|i| i.as_str()),
                ) {
                    let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                    let args: Value =
                        serde_json::from_str(args_str).unwrap_or(json!({}));
                    anthropic_content.push(json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": args
                    }));
                }
            }
        }

        // Usage
        if let Some(u) = response.get("usage") {
            usage = Some(json!({
                "input_tokens": u.get("prompt_tokens").or_else(|| u.get("input_tokens")).and_then(|v| v.as_u64()).unwrap_or(0),
                "output_tokens": u.get("completion_tokens").or_else(|| u.get("output_tokens")).and_then(|v| v.as_u64()).unwrap_or(0),
            }));
        }
    }

    let msg_id = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let mut msg_data = json!({
        "type": "message_start",
        "message": {
            "id": msg_id,
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": anthropic_content,
            "stop_reason": stop_reason,
            "stop_sequence": null,
            "usage": usage.unwrap_or(json!({
                "input_tokens": 0,
                "output_tokens": 0
            }))
        }
    });

    if anthropic_content.is_empty() {
        msg_data["message"]["content"] = json!([]);
    }

    format!(
        r#"event: message_start
data: {}

"#,
        serde_json::to_string(&msg_data).unwrap_or_default()
    )
}
