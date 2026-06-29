# API Routes

> 完整描述 llmproxy 提供的 HTTP API 路由。

---

## 路由总览

| Method | Path | Handler | 说明 |
|--------|------|---------|------|
| POST | `/v1/messages` | `handle_anthropic_messages` | Anthropic Messages API |
| POST | `/v1/messages/{*path}` | `handle_anthropic_messages` | Anthropic（含子路径通配） |
| POST | `/v1/chat/completions` | `handle_openai_chat` | OpenAI Chat Completions |
| POST | `/v1/responses` | `handle_openai_responses` | OpenAI Responses |
| GET/ANY | `/v1/models` | `handle_models` | 模型列表 |
| GET/ANY | `/v1beta/models` | `handle_models` | Gemini 兼容模型列表 |
| ANY | `/v1beta/{*path}` | `handle_gemini` | Gemini API 端点 |
| GET | `/health` | `handle_health` | 健康检查 |
| ANY | `/{*path}` | `handle_fallback` | 404 兜底 |

---

## GET /health

健康检查端点。

**Response**:
```json
{
  "status": "ok",
  "version": "0.0.1",
  "service": "llmproxy"
}
```

---

## GET /v1/models

返回代理可路由的所有模型列表。

**Response** (OpenAI 格式):
```json
{
  "object": "list",
  "data": [
    {
      "id": "deepseek-chat",
      "name": "deepseek",
      "object": "model",
      "created": 0,
      "owned_by": "llmproxy"
    }
  ]
}
```

---

## POST /v1/messages (Anthropic Messages)

接收 Anthropic Messages 格式的请求，根据 `model` 字段路由到对应上游。

**入站格式**: Anthropic Messages API 格式
**出站转换**:
| 上游类型 | 转换路径 | 流式支持 |
|----------|----------|----------|
| `anthropic` | 透传 | ✅ 原生 Anthropic SSE |
| `openai` | Anthropic → OpenAI Chat | ✅ OpenAI SSE → Anthropic SSE |
| `openai_responses` | Anthropic → OpenAI Chat | ✅ (同上) |
| `gemini` | Anthropic → Gemini | ✅ 透传 Gemini SSE |

### 请求体示例

```json
{
  "model": "deepseek-chat",
  "messages": [{"role": "user", "content": "Hello!"}],
  "max_tokens": 4096,
  "stream": true
}
```

---

## POST /v1/chat/completions (OpenAI Chat)

接收 OpenAI Chat Completions 格式的请求。

**出站转换**:
| 上游类型 | 转换路径 | 流式支持 |
|----------|----------|----------|
| `openai` | 透传 | ✅ 原生 OpenAI SSE |
| `openai_responses` | Chat → Responses | ❌ 仅非流式 |
| `anthropic` | OpenAI Chat → Anthropic | ✅ Anthropic SSE → OpenAI SSE |
| `gemini` | OpenAI Chat → Gemini | ✅ 透传 Gemini SSE |

### 请求体示例

```json
{
  "model": "claude-sonnet-4-20250514",
  "messages": [{"role": "user", "content": "Hello!"}],
  "max_tokens": 4096,
  "stream": true
}
```

---

## POST /v1/responses (OpenAI Responses)

接收 OpenAI Responses API 格式的请求。

**出站转换**:
| 上游类型 | 转换路径 | 流式支持 |
|----------|----------|----------|
| `openai_responses` | 透传 | ✅ 原生 SSE |
| `openai` | Responses → Chat | ❌ 仅非流式 |
| `anthropic` | Responses → Anthropic | ✅ Anthropic SSE → OpenAI SSE |
| `gemini` | Responses → Gemini | ✅ 透传 Gemini SSE |

### 请求体

```json
{
  "model": "claude-sonnet-4-20250514",
  "input": "Hello!",
  "max_output_tokens": 4096
}
```

---

## POST/ANY /v1beta/{*path} (Gemini)

接收 Google Gemini API 格式的请求。

**路径校验**: 仅允许 `models/` 和 `tunedModels/` 开头的路径
**模型提取**: 从 path 中查找 `gemini` 前缀的模型名

**出站转换**:
| 上游类型 | 转换路径 | 说明 |
|----------|----------|------|
| `gemini` | 透传 | API key 追加到查询参数 |
| `anthropic/openai` | Gemini → Anthropic | 提取文本内容，构建简单消息 |

---

## 错误响应格式

所有错误返回统一格式：

```json
{
  "error": {
    "type": "error",
    "message": "错误描述"
  }
}
```

常见 HTTP 状态码:
- `400` — 无效请求（模型不存在、模型名非法、转换失败）
- `502` — 上游连接失败
- `504` — 上游超时
- `404` — 未知端点
