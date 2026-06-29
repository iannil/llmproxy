//! YAML 配置文件系统
//!
//! 替代原 CC Switch 的 SQLite 数据库 + ProxyService 组合。
//! 所有上游配置从一个 YAML 文件加载，运行时通过 Arc<RwLock<>> 共享。

use crate::app_config::AppType;
use crate::error::AppError;
use crate::provider::Provider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 顶级配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 代理服务监听地址
    #[serde(default = "default_listen")]
    pub listen: String,

    /// 上游供应商配置列表
    pub upstreams: Vec<UpstreamConfig>,
}

fn default_listen() -> String {
    "127.0.0.1:8880".to_string()
}

/// 单个上游供应商配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    /// 名称（唯一标识）
    pub name: String,

    /// 上游 API 基础 URL
    pub base_url: String,

    /// API 密钥（支持环境变量引用 ${VAR_NAME}）
    pub api_key: String,

    /// 出站协议类型
    #[serde(default = "default_upstream_type")]
    pub r#type: UpstreamType,

    /// 此上游可处理的模型列表
    /// 当入站请求中的 model 匹配此列表时，路由到此上游
    pub models: Vec<String>,

    /// 可选的模型映射（入站模型名 → 上游模型名）
    #[serde(default)]
    pub model_mappings: HashMap<String, String>,

    /// 可选的自定义 Header
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// 上游 API 协议类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpstreamType {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAI,
    #[serde(rename = "openai_responses")]
    OpenAIResponses,
    #[serde(rename = "gemini")]
    Gemini,
}

fn default_upstream_type() -> UpstreamType {
    UpstreamType::OpenAI
}

/// 运行时的共享配置状态
#[derive(Clone)]
pub struct ConfigState {
    inner: Arc<RwLock<ConfigStateInner>>,
}

struct ConfigStateInner {
    /// 从 YAML 加载的原始配置
    config: Config,
    /// 构建好的 Provider 索引（name → Provider）
    providers: HashMap<String, Provider>,
    /// 模型名 → 上游名称 的快速路由表
    model_routes: HashMap<String, String>,
}

impl ConfigState {
    /// 从 YAML 文件加载配置
    pub async fn load(path: &Path) -> Result<Self, AppError> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Io {
                path: path.to_string_lossy().to_string(),
                source: e,
            })?;

        let config: Config = serde_yaml::from_str(&content)
            .map_err(|e| AppError::Config(format!("YAML 解析失败: {}", e)))?;

        Self::from_config(config)
    }

    /// 从 Config 结构构建运行时状态
    pub fn from_config(config: Config) -> Result<Self, AppError> {
        let mut providers = HashMap::new();
        let mut model_routes = HashMap::new();

        for upstream in &config.upstreams {
            // 解析 API Key（支持环境变量引用）
            let api_key = resolve_env_vars(&upstream.api_key);

            let app_type = match upstream.r#type {
                UpstreamType::Anthropic => AppType::Claude,
                UpstreamType::OpenAI | UpstreamType::OpenAIResponses => AppType::Codex,
                UpstreamType::Gemini => AppType::Gemini,
            };

            let provider = Provider::new(
                upstream.name.clone(),
                upstream.base_url.clone(),
                api_key,
                app_type.clone(),
            );

            providers.insert(upstream.name.clone(), provider);

            // 构建模型路由表
            for model in &upstream.models {
                model_routes.insert(model.clone(), upstream.name.clone());
            }
        }

        Ok(Self {
            inner: Arc::new(RwLock::new(ConfigStateInner {
                config,
                providers,
                model_routes,
            })),
        })
    }

    /// 根据模型名查找对应的 Provider
    pub async fn find_provider(&self, model: &str) -> Option<Provider> {
        let inner = self.inner.read().await;
        // 精确匹配
        if let Some(upstream_name) = inner.model_routes.get(model) {
            return inner.providers.get(upstream_name).cloned();
        }
        // 前缀匹配（如 claude-sonnet-4-20250514 匹配 claude-*）
        for (pattern, upstream_name) in &inner.model_routes {
            if model.starts_with(pattern.trim_end_matches('*'))
                || pattern.starts_with(model)
            {
                return inner.providers.get(upstream_name).cloned();
            }
        }
        None
    }

    /// 获取所有 Provider
    pub async fn all_providers(&self) -> Vec<Provider> {
        let inner = self.inner.read().await;
        inner.providers.values().cloned().collect()
    }

    /// 获取监听地址
    pub async fn listen_addr(&self) -> String {
        let inner = self.inner.read().await;
        inner.config.listen.clone()
    }

    /// 获取上游超时（秒）
    pub async fn upstream_timeout_secs(&self) -> u64 {
        300 // default, can be extended in config
    }

    /// 热重载配置
    pub async fn reload(&self, path: &Path) -> Result<(), AppError> {
        let new_state = Self::load(path).await?;
        let new_inner = new_state.inner.read().await;
        let mut inner = self.inner.write().await;
        inner.config = new_inner.config.clone();
        inner.providers = new_inner.providers.clone();
        inner.model_routes = new_inner.model_routes.clone();
        Ok(())
    }

    /// 获取模型映射（入站模型名 → 上游模型名）
    pub async fn get_model_mapping(&self, upstream_name: &str, model: &str) -> Option<String> {
        let inner = self.inner.read().await;
        if let Some(upstream) = inner.config.upstreams.iter().find(|u| u.name == upstream_name) {
            return upstream.model_mappings.get(model).cloned();
        }
        None
    }
}

/// 解析环境变量引用 ${VAR_NAME}
fn resolve_env_vars(s: &str) -> String {
    let mut result = s.to_string();
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    for cap in re.captures_iter(s) {
        if let Some(env_val) = std::env::var(&cap[1]).ok() {
            result = result.replace(&cap[0], &env_val);
        }
    }
    result
}
