# Usage Guide — llmproxy

> 从零开始的使用教程，涵盖配置、运行、测试和常见场景。

---

## 目录

1. [快速开始：从编译到运行](#1-快速开始从编译到运行)
2. [场景一：Claude Code 使用 DeepSeek](#2-场景一claude-code-使用-deepseek)
3. [场景二：Codex 使用 Claude 官方](#3-场景二codex-使用-claude-官方)
4. [场景三：任意 OpenAI 客户端使用 Gemini](#4-场景三任意-openai-客户端使用-gemini)
5. [场景四：多上游混合路由](#5-场景四多上游混合路由)
6. [测试代理是否正常工作](#6-测试代理是否正常工作)
7. [常见问题排查](#7-常见问题排查)
8. [进阶技巧](#8-进阶技巧)

---

## 1. 快速开始：从编译到运行

### 前置条件

- Rust 工具链（MSRV 1.85.0+）：[rustup.rs](https://rustup.rs)
- 一个或多个 AI API 的密钥

### 步骤

#### 1.1 编译

```sh
cd llmproxy
cargo build --release
```

编译产物在 `target/release/llmproxy`。

#### 1.2 配置

复制示例配置并编辑：

```sh
cp config.example.yaml config.yaml
```

编辑 `config.yaml`，填入你的 API Key（支持环境变量）：

```yaml
upstreams:
  - name: deepseek
    base_url: https://api.deepseek.com
    api_key: "${DEEPSEEK_API_KEY}"   # 从环境变量读取
    type: openai
    models:
      - deepseek-chat
```

设置环境变量：

```sh
export DEEPSEEK_API_KEY=sk-xxxxxxxxxxxx
```

#### 1.3 启动

```sh
# 使用默认配置（config.yaml）
cargo run --release

# 或指定配置文件和监听地址
cargo run --release -- --config /path/to/config.yaml --listen 0.0.0.0:8880
```

启动成功后看到日志：

```
INFO  llmproxy] llmproxy starting with 1 upstream(s) on 127.0.0.1:8880
INFO  llmproxy]   → deepseek (deepseek)
INFO  llmproxy::proxy::server] Proxy server listening on 127.0.0.1:8880
```

---

## 2. 场景一：Claude Code 使用 DeepSeek

**目标**：用 Claude Code CLI 工具，但背后实际调用 DeepSeek 的 API。

### 原理

```
Claude Code ──POST /v1/messages──→ llmproxy ──POST /v1/chat/completions──→ DeepSeek API
  (Anthropic 格式)                    (协议转换)                     (OpenAI Chat 格式)
```

### 配置

```yaml
listen: "127.0.0.1:8880"

upstreams:
  - name: deepseek
    base_url: https://api.deepseek.com
    api_key: "${DEEPSEEK_API_KEY}"
    type: openai
    models:
      - deepseek-chat
      - deepseek-reasoner
```

### 设置 Claude Code

```sh
# 告诉 Claude Code 使用本地代理
export ANTHROPIC_BASE_URL=http://127.0.0.1:8880
export ANTHROPIC_API_KEY=not-needed   # llmproxy 不校验此密钥

# 启动 Claude Code，指定模型
claude --model deepseek-chat
```

### 验证

用 curl 模拟 Claude Code 的请求：

```sh
curl -X POST http://127.0.0.1:8880/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: not-needed" \
  -d '{
    "model": "deepseek-chat",
    "messages": [{"role": "user", "content": "Say hello"}],
    "max_tokens": 100
  }'
```

成功时应返回流式 SSE 响应，格式为 Anthropic Messages API。

---

## 3. 场景二：Codex 使用 Claude 官方

**目标**：用 OpenAI Chat 兼容的工具（Codex、OpenAI Python SDK 等）调用 Claude 官方 API。

### 原理

```
Codex ──POST /v1/chat/completions──→ llmproxy ──POST /v1/messages──→ Anthropic API
  (OpenAI Chat 格式)                     (协议转换)                (Anthropic 格式)
```

### 配置

```yaml
listen: "127.0.0.1:8880"

upstreams:
  - name: claude-official
    base_url: https://api.anthropic.com
    api_key: "${ANTHROPIC_API_KEY}"
    type: anthropic
    models:
      - claude-sonnet-4-20250514
      - claude-opus-4-20250514
```

### 使用 OpenAI Python SDK

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8880",  # → llmproxy
    api_key="not-needed",               # llmproxy 不校验
)

response = client.chat.completions.create(
    model="claude-sonnet-4-20250514",   # 实际上是 Claude
    messages=[{"role": "user", "content": "Hello!"}],
)
print(response.choices[0].message.content)
```

### 验证

```sh
curl -X POST http://127.0.0.1:8880/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer not-needed" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "messages": [{"role": "user", "content": "Say hello in Japanese"}],
    "max_tokens": 100,
    "stream": true
  }'
```

成功时应返回 OpenAI Chat 格式的 SSE 流。

---

## 4. 场景三：任意 OpenAI 客户端使用 Gemini

**目标**：用 OpenAI Chat 兼容的工具调用 Google Gemini。

### 原理

```
App ──POST /v1/chat/completions──→ llmproxy ──POST /v1beta/models/...──→ Gemini API
  (OpenAI Chat 格式)                    (协议转换)                    (Gemini 格式)
```

### 配置

```yaml
listen: "127.0.0.1:8880"

upstreams:
  - name: gemini
    base_url: https://generativelanguage.googleapis.com
    api_key: "${GEMINI_API_KEY}"
    type: gemini
    models:
      - gemini-2.5-flash
      - gemini-2.5-pro
```

### 验证

```sh
curl -X POST http://127.0.0.1:8880/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer not-needed" \
  -d '{
    "model": "gemini-2.5-flash",
    "messages": [{"role": "user", "content": "请用中文回复"}],
    "max_tokens": 100
  }'
```

---

## 5. 场景四：多上游混合路由

**目标**：同时配置多个上游，通过不同的 `model` 值路由到不同供应商。

### 配置

```yaml
listen: "127.0.0.1:8880"

upstreams:
  # 用 DeepSeek 处理便宜模型
  - name: deepseek
    base_url: https://api.deepseek.com
    api_key: "${DEEPSEEK_API_KEY}"
    type: openai
    models:
      - deepseek-chat
      - deepseek-reasoner

  # Claude 处理复杂任务
  - name: claude-official
    base_url: https://api.anthropic.com
    api_key: "${ANTHROPIC_API_KEY}"
    type: anthropic
    models:
      - claude-sonnet-4-20250514
      - claude-opus-4-20250514
    model_mappings:
      claude-sonnet-4-20250514: claude-sonnet-4-20250514

  # Gemini 备用
  - name: gemini
    base_url: https://generativelanguage.googleapis.com
    api_key: "${GEMINI_API_KEY}"
    type: gemini
    models:
      - gemini-2.5-flash
      - gemini-2.5-pro
```

### 使用方式

```sh
# Claude Code 用 DeepSeek
claude --model deepseek-chat

# Claude Code 用 Claude 官方
claude --model claude-sonnet-4-20250514

# Codex 用 Gemini
codex --model gemini-2.5-flash
#（需要配置 Codex 的 base_url 为 http://127.0.0.1:8880）
```

---

## 6. 测试代理是否正常工作

### 健康检查

```sh
curl http://127.0.0.1:8880/health
# → {"status":"ok","version":"0.0.1","service":"llmproxy"}
```

### 模型列表

```sh
curl http://127.0.0.1:8880/v1/models
# → {"object":"list","data":[{"id":"deepseek","name":"deepseek",...}]}
```

### 快速连通性测试

用非流式请求测试转换链路：

```sh
# 测试 Anthropic → OpenAI 转换（Claude Code 用 DeepSeek）
curl -s -X POST http://127.0.0.1:8880/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: test" \
  -d '{"model":"deepseek-chat","messages":[{"role":"user","content":"Hi"}],"max_tokens":50}' \
  | head -c 200

# 测试 OpenAI → Anthropic 转换（Codex 用 Claude）
curl -s -X POST http://127.0.0.1:8880/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer test" \
  -d '{"model":"claude-sonnet-4-20250514","messages":[{"role":"user","content":"Hi"}],"max_tokens":50}' \
  | head -c 200
```

---

## 7. 常见问题排查

### 7.1 启动报错 "config.yaml: No such file or directory"

**原因**：当前目录下没有 `config.yaml`。

**解决**：
```sh
cp config.example.yaml config.yaml
# 或使用 --config 指定路径
cargo run --release -- --config /path/to/your-config.yaml
```

### 7.2 启动 panic "catch-all parameters are only allowed at the end of a route"

**原因**：项目使用了旧版 matchit 路由语法 `{*path}`，但实际依赖的是 matchit 0.7（需 `/*path`）。

**解决**：此问题已在 v0.0.1 修复。如遇到，将 `server.rs` 中的 `/{*path}` 替换为 `/*path`。

### 7.3 请求返回 "没有为模型 'xxx' 配置上游"

**原因**：配置中的 `models` 列表不含请求中的 `model` 值。

**解决**：检查 `config.yaml` 中 `upstreams[].models` 是否包含你使用的模型名，或调整客户端的 `--model` 参数。

### 7.4 流式响应卡住/不完整

**可能原因**：
- 上游超时（默认 300s）
- 客户端与服务端之间的网络问题
- 某些上游不支持流式请求（检查请求中是否包含 `"stream": true`）

### 7.5 API Key 不生效

**原因**：配置中的 `${VAR_NAME}` 环境变量未设置。

**解决**：
```sh
# 检查环境变量是否已设置
echo $DEEPSEEK_API_KEY

# 如未设置，导出密钥
export DEEPSEEK_API_KEY=sk-xxxxxxxxxxxx
```

### 7.6 macOS 端口被占用

```sh
# 查看端口占用
lsof -i :8880

# 使用其他端口启动
cargo run --release -- --listen 127.0.0.1:8081
```

---

## 8. 进阶技巧

### 8.1 后台运行

```sh
# nohup 方式
nohup ./target/release/llmproxy --config config.yaml > llmproxy.log 2>&1 &

# 查看日志
tail -f llmproxy.log
```

### 8.2 模型映射（model_mappings）

当客户端的模型名与上游模型名不同时使用：

```yaml
- name: my-provider
  base_url: https://api.my-provider.com
  api_key: "${MY_API_KEY}"
  type: openai
  models:
    - gpt-4o-mini     # 客户端请求的模型名
  model_mappings:
    gpt-4o-mini: my-model-v2  # → 实际发送到上游的模型名
```

### 8.3 自定义请求头

```yaml
- name: custom-provider
  base_url: https://api.custom.com
  api_key: "${CUSTOM_API_KEY}"
  type: openai
  models:
    - custom-model
  headers:
    X-Organization-ID: "org-xxx"
    X-Source: "llmproxy"
```

### 8.4 监听所有网络接口

```sh
cargo run --release -- --listen 0.0.0.0:8880
```

> ⚠️ 注意：llmproxy 无内置认证。监听 `0.0.0.0` 时请确保网络环境安全（如防火墙限制），或用反向代理（如 Nginx）添加认证层。

### 8.5 连接池与性能

llmproxy 使用连接池（32 空闲连接，300s 超时），默认配置适合单用户使用。高并发场景无需额外调优。

### 8.6 认证方式：ANTHROPIC_AUTH_TOKEN

llmproxy 自动识别 Anthropic 上游的认证方式：

- `api_key` 以 `sk-ant-` 开头 → 发送 `x-api-key` 头（传统 API key）
- 其他格式（包括 OAuth token 等）→ 发送 `Authorization: Bearer` 头

配置示例（使用 ANTHROPIC_AUTH_TOKEN）：

```yaml
- name: claude-official
  base_url: https://api.anthropic.com
  api_key: "${ANTHROPIC_AUTH_TOKEN}"
  type: anthropic
  models:
    - claude-sonnet-4-20250514
```

环境变量设置优先顺序：`ANTHROPIC_AUTH_TOKEN` > `ANTHROPIC_API_KEY`。
