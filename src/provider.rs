use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::app_config::AppType;

/// 供应商核心数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    #[serde(rename = "settingsConfig")]
    pub settings_config: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ProviderMeta>,
}

impl Provider {
    /// 从简单字段创建 Provider（lite 版本使用）
    pub fn new(id: String, base_url: String, api_key: String, app_type: AppType) -> Self {
        // 构建 settings_config JSON（适配 adapter 期望的格式）
        let settings_config = match app_type {
            AppType::Claude => json!({
                "env": {
                    "ANTHROPIC_BASE_URL": base_url,
                    "ANTHROPIC_API_KEY": api_key,
                }
            }),
            AppType::Codex => json!({
                "baseUrl": base_url,
                "apiKey": api_key,
            }),
            AppType::Gemini => json!({
                "env": {
                    "GEMINI_API_KEY": api_key,
                    "GEMINI_BASE_URL": base_url,
                }
            }),
        };

        let api_format = match app_type {
            AppType::Claude => "anthropic",
            AppType::Codex => "openai_chat",
            AppType::Gemini => "gemini_native",
        };

        Self {
            id: id.clone(),
            name: id,
            settings_config,
            meta: Some(ProviderMeta {
                api_format: Some(api_format.to_string()),
            }),
        }
    }

    pub fn display_name(&self) -> &str {
        &self.name
    }
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            settings_config: json!({}),
            meta: None,
        }
    }
}

/// 供应商元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMeta {
    /// API 格式：anthropic, openai_chat, openai_responses, gemini_native
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "apiFormat")]
    pub api_format: Option<String>,
}




