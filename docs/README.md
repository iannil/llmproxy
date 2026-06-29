# llmproxy — Documentation Index

> **Version**: 0.0.1 | **Language**: Rust (edition 2021) | **License**: MIT
>
> 精简版模型 API 协议桥接代理。从原 CC Switch Tauri 桌面应用剥离，只保留核心代理功能。

---

## Quick Start

```sh
# 构建
cargo build --release

# 运行（使用默认 config.yaml）
cargo run --release

# 指定配置文件和监听地址
cargo run --release -- --config config.yaml --listen 0.0.0.0:8880
```

## Documentation Structure

| Document | Description |
|----------|-------------|
| **[usage-guide.md](usage-guide.md)** | 从零开始的使用教程（含场景示例、测试、排错） |
| **[architecture.md](architecture.md)** | 项目架构、模块划分、设计决策 |
| **[config-format.md](config-format.md)** | YAML 配置文件格式详解 |
| **[api-routes.md](api-routes.md)** | 所有 API 路由和请求/响应格式 |
| **[conversion-paths.md](conversion-paths.md)** | 协议转换路径和状态 |
| **[progress.md](progress.md)** | 当前项目进展和状态 |
| **[CHANGELOG.md](CHANGELOG.md)** | 版本变更历史 |
| **PLANS/roadmap.md** | 未来规划和路线图 |

## Key Facts

- **~2,325 行** Rust 代码（不含依赖）
- **无外部数据库**：所有配置来源于 YAML 文件
- **无 Tauri/前端**：纯 CLI 服务
- **无会话/用量追踪**：专注于协议转换
- **连接池**: 32 个空闲连接，300s 超时

## Supported Protocols

| Protocol | Direction | Streaming |
|----------|-----------|-----------|
| Anthropic Messages ↔ OpenAI Chat | 双向 | ✅ |
| OpenAI Chat ↔ OpenAI Responses | 双向 | ❌ (仅非流式) |
| Anthropic → Gemini | 单向 | ❌ (仅非流式) |
| Gemini → Anthropic/OpenAI | 单向 | ❌ (仅非流式) |
