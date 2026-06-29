//! SSE streaming conversion
//!
//! Converts stream chunks between Anthropic and OpenAI SSE formats on-the-fly.
//!
//! ## Key paths
//!
//! ### Anthropic inbound → OpenAI Chat outbound (Claude Code using OpenAI upstream)
//! OpenAI SSE chunks → Anthropic SSE events
//!
//! ### OpenAI Chat inbound → Anthropic outbound (Codex using Anthropic upstream)
//! Anthropic SSE events → OpenAI SSE chunks

use crate::proxy::error::ProxyError;
use serde_json::Value;

/// Stateful converter for OpenAI Chat SSE → Anthropic SSE
///
/// Maintains state across chunks to handle the multi-event Anthropic protocol
/// (message_start → content_block_start → content_block_delta* → content_block_stop → message_stop).
pub struct OpenAiToAnthropicStream {
    model: String,
    started: bool,
    content_block_started: bool,
    buffer: String,
}

impl OpenAiToAnthropicStream {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            started: false,
            content_block_started: false,
            buffer: String::new(),
        }
    }

    /// Process one chunk of OpenAI SSE data, return zero or more Anthropic SSE events
    pub fn push(&mut self, chunk: &str) -> Result<String, ProxyError> {
        let mut output = String::new();
        self.buffer.push_str(chunk);

        // Process complete SSE messages (delimited by \n\n)
        loop {
            let end = match self.buffer.find("\n\n") {
                Some(pos) => pos,
                None => break,
            };

            let event = self.buffer[..end].trim().to_string();
            self.buffer = self.buffer[end + 2..].to_string();

            // Extract data content (after "data: ")
            let data = match event.strip_prefix("data: ") {
                Some(d) => d.trim(),
                None => continue,
            };

            // Skip [DONE] sentinel
            if data == "[DONE]" {
                output.push_str(&self.emit_message_stop());
                continue;
            }

            // Parse JSON
            let val: Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let choices = match val["choices"].as_array() {
                Some(c) if !c.is_empty() => c,
                _ => continue,
            };
            let choice = &choices[0];
            let delta = match choice.get("delta") {
                Some(d) => d,
                None => continue,
            };
            let finish_reason = choice.get("finish_reason").and_then(|f| f.as_str());

            // Emit message_start on first meaningful delta
            if !self.started {
                if delta.get("role").is_some() || delta.get("content").is_some() {
                    output.push_str(&self.emit_message_start());
                }
            }

            // Content delta
            if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                if !text.is_empty() {
                    if !self.content_block_started {
                        output.push_str("event: content_block_start\ndata: ");
                        output.push_str(&serde_json::to_string(&serde_json::json!({
                            "type": "content_block_start",
                            "index": 0,
                            "content_block": {"type": "text", "text": ""}
                        })).unwrap_or_default());
                        output.push_str("\n\n");
                        self.content_block_started = true;
                    }
                    output.push_str("event: content_block_delta\ndata: ");
                    output.push_str(&serde_json::to_string(&serde_json::json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": {"type": "text_delta", "text": text}
                    })).unwrap_or_default());
                    output.push_str("\n\n");
                }
            }

            // Tool calls
            if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tcs {
                    if let (Some(name), Some(id)) = (
                        tc["function"]["name"].as_str(),
                        tc.get("id").and_then(|i| i.as_str()),
                    ) {
                        let args = tc["function"]["arguments"].as_str().unwrap_or("{}");
                        output.push_str("event: content_block_start\ndata: ");
                        output.push_str(&serde_json::to_string(&serde_json::json!({
                            "type": "content_block_start",
                            "index": 0,
                            "content_block": {
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": {}
                            }
                        })).unwrap_or_default());
                        output.push_str("\n\n");
                        output.push_str("event: content_block_delta\ndata: ");
                        output.push_str(&serde_json::to_string(&serde_json::json!({
                            "type": "content_block_delta",
                            "index": 0,
                            "delta": {
                                "type": "input_json_delta",
                                "partial_json": args
                            }
                        })).unwrap_or_default());
                        output.push_str("\n\n");
                    }
                }
            }

            // Finish reason
            if let Some(reason) = finish_reason {
                if reason == "stop" || reason == "end_turn" {
                    if self.content_block_started {
                        output.push_str("event: content_block_stop\ndata: ");
                        output.push_str(&serde_json::to_string(&serde_json::json!({
                            "type": "content_block_stop",
                            "index": 0
                        })).unwrap_or_default());
                        output.push_str("\n\n");
                    }
                    output.push_str(&self.emit_message_stop());
                } else if reason == "tool_calls" {
                    if self.content_block_started {
                        output.push_str("event: content_block_stop\ndata: ");
                        output.push_str(&serde_json::to_string(&serde_json::json!({
                            "type": "content_block_stop",
                            "index": 0
                        })).unwrap_or_default());
                        output.push_str("\n\n");
                    }
                    output.push_str(&self.emit_message_stop());
                } else if reason == "length" {
                    if self.content_block_started {
                        output.push_str("event: content_block_stop\ndata: ");
                        output.push_str(&serde_json::to_string(&serde_json::json!({
                            "type": "content_block_stop",
                            "index": 0
                        })).unwrap_or_default());
                        output.push_str("\n\n");
                    }
                    output.push_str(&self.emit_message_stop());
                }
            }
        }

        Ok(output)
    }

    fn emit_message_start(&mut self) -> String {
        self.started = true;
        let msg_id = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        format!(
            "event: message_start\ndata: {}\n\n",
            serde_json::to_string(&serde_json::json!({
                "type": "message_start",
                "message": {
                    "id": msg_id,
                    "type": "message",
                    "role": "assistant",
                    "model": self.model,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": {"input_tokens": 0, "output_tokens": 0}
                }
            })).unwrap_or_default()
        )
    }

    fn emit_message_stop(&mut self) -> String {
        self.started = false;
        self.content_block_started = false;
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string()
    }
}

/// Stateful converter for Anthropic SSE → OpenAI Chat SSE
///
/// Converts Anthropic SSE events back to OpenAI Chat Completions streaming format.
pub struct AnthropicToOpenAiStream {
    model: String,
    current_tool_name: Option<String>,
    current_tool_id: Option<String>,
}

impl AnthropicToOpenAiStream {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            current_tool_name: None,
            current_tool_id: None,
        }
    }

    /// Process one chunk of Anthropic SSE data, return zero or more OpenAI SSE chunks
    pub fn push(&mut self, chunk: &str) -> Result<String, ProxyError> {
        let mut output = String::new();
        let mut buffer = chunk.to_string();

        loop {
            // Find event + data boundary (\n\n)
            let end = match buffer.find("\n\n") {
                Some(pos) => pos,
                None => break,
            };

            let event_block = buffer[..end].to_string();
            buffer = buffer[end + 2..].to_string();

            let lines: Vec<&str> = event_block.lines().collect();
            if lines.len() < 2 {
                continue;
            }

            let _event_type = lines[0].strip_prefix("event: ").unwrap_or("");
            let data_line = lines[1].strip_prefix("data: ").unwrap_or("");

            let val: Value = match serde_json::from_str(data_line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let data_type = val["type"].as_str().unwrap_or("");

            match data_type {
                "message_start" => {
                    // Emit OpenAI role chunk
                    let model = val["message"]["model"].as_str().unwrap_or(&self.model);
                    output.push_str(&format!(
                        "data: {}\n\n",
                        serde_json::to_string(&serde_json::json!({
                            "choices": [{
                                "index": 0,
                                "delta": {"role": "assistant", "content": ""},
                                "finish_reason": null
                            }],
                            "model": model
                        })).unwrap_or_default()
                    ));
                }
                "content_block_start" => {
                    let block = &val["content_block"];
                    match block["type"].as_str() {
                        Some("tool_use") => {
                            self.current_tool_name = block["name"].as_str().map(|s| s.to_string());
                            self.current_tool_id =
                                block["id"].as_str().map(|s| s.to_string());
                            // Emit tool_call start
                            let id = self.current_tool_id.as_deref().unwrap_or("toolu_000");
                            let name = self.current_tool_name.as_deref().unwrap_or("");
                            output.push_str(&format!(
                                "data: {}\n\n",
                                serde_json::to_string(&serde_json::json!({
                                    "choices": [{
                                        "index": 0,
                                        "delta": {
                                            "tool_calls": [{
                                                "index": 0,
                                                "id": id,
                                                "type": "function",
                                                "function": {
                                                    "name": name,
                                                    "arguments": ""
                                                }
                                            }]
                                        },
                                        "finish_reason": null
                                    }],
                                    "model": self.model
                                })).unwrap_or_default()
                            ));
                        }
                        _ => {} // text block start is implicit
                    }
                }
                "content_block_delta" => {
                    let delta = &val["delta"];
                    match delta["type"].as_str() {
                        Some("text_delta") => {
                            let text = delta["text"].as_str().unwrap_or("");
                            output.push_str(&format!(
                                "data: {}\n\n",
                                serde_json::to_string(&serde_json::json!({
                                    "choices": [{
                                        "index": 0,
                                        "delta": {"content": text},
                                        "finish_reason": null
                                    }],
                                    "model": self.model
                                })).unwrap_or_default()
                            ));
                        }
                        Some("input_json_delta") => {
                            let partial = delta["partial_json"].as_str().unwrap_or("");
                            output.push_str(&format!(
                                "data: {}\n\n",
                                serde_json::to_string(&serde_json::json!({
                                    "choices": [{
                                        "index": 0,
                                        "delta": {
                                            "tool_calls": [{
                                                "index": 0,
                                                "function": {"arguments": partial}
                                            }]
                                        },
                                        "finish_reason": null
                                    }],
                                    "model": self.model
                                })).unwrap_or_default()
                            ));
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    // Nothing to emit; stop will be on message_stop
                }
                "message_stop" => {
                    // Emit finish_reason
                    output.push_str(&format!(
                        "data: {}\n\n",
                        serde_json::to_string(&serde_json::json!({
                            "choices": [{
                                "index": 0,
                                "delta": {},
                                "finish_reason": if self.current_tool_name.is_some() {
                                    "tool_calls"
                                } else {
                                    "stop"
                                }
                            }],
                            "model": self.model
                        })).unwrap_or_default()
                    ));
                    output.push_str("data: [DONE]\n\n");
                    self.current_tool_name = None;
                    self.current_tool_id = None;
                }
                "ping" => {
                    // Keep-alive, ignore
                }
                "error" => {
                    let error_msg = val.get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("unknown error");
                    output.push_str(&format!(
                        "data: {{\"error\":{{\"message\":\"{error_msg}\"}}}}\n\n"
                    ));
                }
                _ => {}
            }
        }

        Ok(output)
    }
}
