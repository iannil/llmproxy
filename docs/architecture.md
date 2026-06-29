# Architecture

> 详细描述 llmproxy 的架构设计、模块划分和数据流。

---

## 项目结构概览

```
src/
├── lib.rs                # 模块声明（5 个顶层模块）
├── main.rs               # CLI 入口（clap 参数解析 + 服务器启动）
├── config.rs             # YAML 配置加载 + 模型路由
├── provider.rs           # Provider 数据结构
├── app_config.rs         # AppType 枚举（Claude/Codex/Gemini）
├── error.rs              # AppError 全局错误类型
└── proxy/               # 核心代理逻辑
    ├── mod.rs            # 子模块声明
    ├── server.rs         # Axum HTTP 服务器（路由 + 请求转发）
    ├── convert.rs        # 协议转换：Anthropic ↔ OpenAI Chat ↔ Responses
    ├── gemini.rs         # 协议转换：Anthropic ↔ Gemini
    ├── streaming.rs      # SSE 流式转换状态机
    ├── forwarder.rs      # 上游解析、认证头、URL 构建
    ├── types.rs          # UpstreamProtocol / UpstreamInfo
    └── error.rs          # ProxyError 错误类型
```

## 数据流

### 请求生命周期

```
CLI 工具 (Claude Code / Codex / Gemini CLI)
    │
    ▼  POST 请求 (Anthropic / OpenAI / Gemini 格式)
┌─────────────────────────────────────────────┐
│  Axum Server (server.rs)                    │
│  ├── 模型名校验 (validate_model_name)        │
│  ├── 上游解析 (resolve_upstream → config)    │
│  ├── 协议转换 (convert / gemini 模块)          │
│  └── 请求转发 (forward_json)                  │
└─────────────────────────────────────────────┘
    │
    ▼ 转换后的请求 (Anthropic / OpenAI / Gemini 格式)
┌─────────────────────────────────────────────┐
│  上游 API (DeepSeek / Claude 官方 / Gemini)  │
└─────────────────────────────────────────────┘
    │
    ▼ 响应 (JSON 或 SSE 流)
┌─────────────────────────────────────────────┐
│  SSE 流式转换 (streaming.rs)  ← 如需要       │
│  ├── OpenAiToAnthropicStream                │
│  └── AnthropicToOpenAiStream                │
└─────────────────────────────────────────────┘
    │
    ▼ 转换后的响应
 CLI 工具
```

## 关键设计决策

### 1. 纯 YAML 配置，无数据库

所有上游配置、模型映射从单个 YAML 文件加载，运行时通过 `Arc<RwLock<>>` 共享。

**优势**：零部署依赖，配置即代码。
**劣势**：不支持运行时动态增删上游（仅支持 `reload` 热重载）。

### 2. 无状态代理

不存储任何会话状态、不做用量追踪、无认证层。以透明代理模式运行。

### 3. 双转换路径

`convert.rs` 处理 Anthropic ↔ OpenAI Chat ↔ Responses 的转换。
`gemini.rs` 处理 Anthropic ↔ Gemini 的转换。
两个模块保持独立，通过 `server.rs` 中的匹配逻辑选择转换路径。

### 4. SSE 流式转换基于状态机

`streaming.rs` 中的 `OpenAiToAnthropicStream` 和 `AnthropicToOpenAiStream` 使用状态机模式，
跨多个异步块维护解析状态，而非正则替换。

### 5. 模型名注入防护

所有入站请求的 `model` 字段通过 `validate_model_name()` 校验，
拒绝包含路径遍历字符（`..`、`/`、`\`、`#`、`?`）的模型名。

### 6. 认证头按协议类型分派

- **Anthropic**: `x-api-key` + `anthropic-version: 2023-06-01`
- **OpenAI**: `Authorization: Bearer <key>`
- **Gemini**: API key 放入查询字符串

## 路由表

| Method | Path | Handler | 说明 |
|--------|------|---------|------|
| POST | `/v1/messages` | `handle_anthropic_messages` | Anthropic Messages API |
| POST | `/v1/messages/{*path}` | `handle_anthropic_messages` | Anthropic（含子路径） |
| POST | `/v1/chat/completions` | `handle_openai_chat` | OpenAI Chat API |
| POST | `/v1/responses` | `handle_openai_responses` | OpenAI Responses API |
| GET | `/v1/models` | `handle_models` | 模型列表 |
| GET | `/v1beta/models` | `handle_models` | Gemini 兼容模型列表 |
| ANY | `/v1beta/{*path}` | `handle_gemini` | Gemini API |
| GET | `/health` | `handle_health` | 健康检查 |
| ANY | `/{*path}` | `handle_fallback` | 404 兜底 |

## 依赖

所有依赖通过 Cargo.toml 管理，无外部系统依赖。关键依赖：

- **tokio + axum** — 异步运行时 + HTTP 服务器
- **reqwest** — HTTP 客户端（连接池 32，rustls TLS）
- **serde + serde_yaml** — 序列化 + YAML 解析
- **clap** — CLI 参数解析
- **regex** — 环境变量解析
- **uuid** — 消息 ID 生成

---

## 模块间接口

### ConfigState (config.rs)

```rust
pub async fn find_provider(&self, model: &str) -> Option<Provider>
pub async fn all_providers(&self) -> Vec<Provider>
pub async fn get_model_mapping(&self, upstream_name: &str, model: &str) -> Option<String>
pub async fn reload(&self, path: &Path) -> Result<(), AppError>
```

### UpstreamInfo (types.rs)

```rust
pub struct UpstreamInfo {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub protocol: UpstreamProtocol,  // Anthropic | OpenAI | OpenAIResponses | Gemini
    pub model_mappings: HashMap<String, String>,
    pub custom_headers: HashMap<String, String>,
}
```

### SSE 转换器接口 (streaming.rs)

两个转换器共享统一接口：
```rust
pub fn push(&mut self, chunk: &str) -> Result<String, ProxyError>
```
入参是一段可能包含 0~N 个完整 SSE 事件的原始块，返回值是通过转换后的 SSE 输出。
