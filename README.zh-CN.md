# llmproxy

> **统一模型 API 桥接代理** — 任意 AI 工具无缝对接任意模型供应商，协议自动转换。

[English](README.md)

[![Crates.io](https://img.shields.io/crates/v/llmproxy.svg)](https://crates.io/crates/llmproxy)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
![Rust](https://img.shields.io/badge/rust-1.85.0+-blue.svg)

```bash
# Claude Code → DeepSeek
export ANTHROPIC_BASE_URL=http://localhost:8880
claude --model deepseek-chat

# Codex → Claude
codex --model claude-sonnet-4-20250514

# 任意 OpenAI 客户端 → Gemini
curl http://localhost:8880/v1/chat/completions \
  -d '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"你好"}]}'
```

llmproxy 位于你的 AI CLI 工具与上游模型 API 之间，实时翻译请求和流式响应。无数据库、无会话——仅一个轻量 YAML 配置文件。

---

## 特性

- **多协议支持**：Anthropic Messages、OpenAI Chat、OpenAI Responses、Google Gemini
- **双向流式转换**：Anthropic ↔ OpenAI SSE（生产就绪）
- **单文件 YAML 配置**：无数据库、无 GUI、零复杂度
- **模型路由**：精确匹配 + 前缀匹配，多上游自动分发
- **模型映射**：按上游重写模型名
- **环境变量注入**：配置中 `${API_KEY}` 自动替换
- **安全防护**：模型名注入校验

### 协议转换矩阵

| 入站 → 出站 | Anthropic | OpenAI Chat | Responses | Gemini |
|------------|-----------|-------------|-----------|--------|
| Anthropic Messages | ✅ 透传 | ✅ + 流式 | ✅ | ⚠️ 基础 |
| OpenAI Chat | ✅ + 流式 | ✅ 透传 | ✅ | ⚠️ 基础 |
| OpenAI Responses | ✅ | ✅ | ✅ 透传 | ⚠️ 基础 |
| Gemini | ✅ 简化 | ✅ 简化 | ✅ 简化 | ✅ 透传 |

---

## 快速开始

```bash
# 1. 编译
cargo build --release

# 2. 配置
cp config.example.yaml config.yaml
# 编辑 config.yaml，填入 API Key

# 3. 运行
cargo run --release
```

默认监听地址：`127.0.0.1:8880`（可用 `--listen` 覆盖）。

### 配置文件

```yaml
listen: "127.0.0.1:8880"

upstreams:
  - name: deepseek
    base_url: https://api.deepseek.com
    api_key: "${DEEPSEEK_API_KEY}"    # 支持环境变量引用
    type: openai                       # anthropic | openai | openai_responses | gemini
    models:
      - deepseek-chat
```

### CLI 参数

```text
Usage: llmproxy [OPTIONS]

Options:
  -c, --config <FILE>  配置文件路径 [默认: config.yaml]
  -l, --listen <ADDR>  监听地址（覆盖配置文件）
  -h, --help           打印帮助
  -V, --version        打印版本
```

---

## 使用场景

### Claude Code 使用 DeepSeek

```bash
export ANTHROPIC_BASE_URL=http://localhost:8880
export ANTHROPIC_API_KEY=not-needed
claude --model deepseek-chat
```

### Codex / OpenAI SDK 使用 Claude

```python
from openai import OpenAI
client = OpenAI(base_url="http://localhost:8880", api_key="not-needed")
response = client.chat.completions.create(
    model="claude-sonnet-4-20250514",
    messages=[{"role": "user", "content": "你好！"}]
)
```

### 任意命令行使用 Gemini

```bash
curl -X POST http://localhost:8880/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"你好"}],"max_tokens":100}'
```

---

## 路由表

| 方法 | 路径 | 协议 |
|------|------|------|
| POST | `/v1/messages` | Anthropic Messages |
| POST | `/v1/chat/completions` | OpenAI Chat |
| POST | `/v1/responses` | OpenAI Responses |
| GET | `/v1/models` | 模型列表 |
| ANY | `/v1beta/*` | Google Gemini |
| GET | `/health` | 健康检查 |

---

## 文档

| 文档 | 说明 |
|------|------|
| [docs/usage-guide.md](docs/usage-guide.md) | 从零开始的使用教程（含场景、测试、排错） |
| [docs/config-format.md](docs/config-format.md) | YAML 配置参考 |
| [docs/api-routes.md](docs/api-routes.md) | 完整 API 参考 |
| [docs/conversion-paths.md](docs/conversion-paths.md) | 协议转换细节 |
| [docs/architecture.md](docs/architecture.md) | 架构与设计决策 |

---

## 为什么用 llmproxy？

- **约 2,300 行 Rust** — 小巧、可审计、无冗余
- **无外部数据库** — 配置即文件
- **无状态** — 随时重启，不丢会话
- **透明** — API Key 不出你的网络

---

## License

MIT © 2026

> *llmproxy — 桥接任何模型、任何工具、任何协议。*