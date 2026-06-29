//! Proxy types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Upstream protocol type
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum UpstreamProtocol {
    /// Anthropic Messages API
    Anthropic,
    /// OpenAI Chat Completions API
    OpenAI,
    /// OpenAI Responses API
    OpenAIResponses,
    /// Google Gemini API
    Gemini,
}

impl UpstreamProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            UpstreamProtocol::Anthropic => "anthropic",
            UpstreamProtocol::OpenAI => "openai",
            UpstreamProtocol::OpenAIResponses => "openai_responses",
            UpstreamProtocol::Gemini => "gemini",
        }
    }
}

/// Upstream provider info extracted from config
#[derive(Debug, Clone)]
pub struct UpstreamInfo {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub protocol: UpstreamProtocol,
    pub model_mappings: HashMap<String, String>,
    pub custom_headers: HashMap<String, String>,
}

/// Stream chunk from upstream
#[derive(Debug)]
pub enum StreamChunk {
    Data(String),
    Done,
    Error(String),
}
