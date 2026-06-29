# Protocol Conversion Paths

> 详细描述 llmproxy 支持的协议转换路径、状态和限制。

---

## 转换矩阵

### 请求转换

| 入站协议 \ 出站协议 | Anthropic | OpenAI Chat | OpenAI Responses | Gemini |
|---------------------|-----------|-------------|------------------|--------|
| Anthropic Messages | ✅ 透传 | ✅ convert.rs | ✅ convert.rs | ⚠️ gemini.rs |
| OpenAI Chat | ✅ convert.rs | ✅ 透传 | ✅ convert.rs | ⚠️ gemini.rs |
| OpenAI Responses | ✅ convert.rs | ✅ convert.rs | ✅ 透传 | ⚠️ gemini.rs |
| Gemini | ✅ 简化版 | ✅ 简化版 | ✅ 简化版 | ✅ 透传 |

### 流式转换

| 方向 | 状态 | 实现 |
|------|------|------|
| OpenAI SSE → Anthropic SSE | ✅ 生产就绪 | `OpenAiToAnthropicStream` |
| Anthropic SSE → OpenAI SSE | ✅ 生产就绪 | `AnthropicToOpenAiStream` |
| 其他流式路径 | ⚠️ 仅透传 | `Passthrough` |

---

## 转换路径详情

### 1. Anthropic Messages → OpenAI Chat (`convert.rs:83-132`)

**方向**：Claude Code 使用 OpenAI 兼容上游（DeepSeek 等）

**字段映射**：

| Anthropic | OpenAI Chat | 说明 |
|-----------|-------------|------|
| `model` | `model` | 透传 |
| `system` (string) | `messages[0].role=system` | 系统提示 |
| `messages[].role=user` | `messages[].role=user` | 用户消息 |
| `messages[].role=assistant` | `messages[].role=assistant` | 助手消息 |
| `content` (string array) | `content` (string) | 文本合并 |
| `content[].type=tool_use` | `tool_calls[]` | 工具调用转换 |
| `content[].type=tool_result` | `messages[].role=tool` | 工具结果 |
| `tools[]` | `tools[].type=function` | 工具格式转换 |
| `max_tokens` | `max_tokens` | 透传 |
| `temperature` | `temperature` | 透传 |

**限制**：
- image content blocks 未处理（Anthropic 的 image 类型不转换）
- tool_result 的 content 如果是 array 只取第一个 text 块

### 2. OpenAI Chat → Anthropic Messages (`convert.rs:11-121`)

**方向**：Codex 使用 Anthropic 官方上游

**字段映射**：

| OpenAI Chat | Anthropic | 说明 |
|-------------|-----------|------|
| `model` | `model` | 透传 |
| `messages[].role=system` | `system` | 合并为 system 字段 |
| `messages[].role=user` | `messages[].role=user` | 透传 |
| `messages[].role=assistant` | `messages[].role=assistant` | 透传 |
| `messages[].role=tool` | `messages[].role=user` + `type=tool_result` | 角色变更 |
| `tool_calls[]` | `content[].type=tool_use` | 工具调用转换 |
| `tools[].function` | `tools[]` | 工具格式转换 |
| `max_tokens` | `max_tokens` | 透传 |

**注意**：
- tool 类型的消息在 Anthropic 中被转为 role=user 的 tool_result content block

### 3. Chat ↔ Responses (`convert.rs:295-370`)

**方向**：OpenAI Chat 和 OpenAI Responses 协议互转

**限制**：仅支持非流式

**Chat → Responses**:
- `messages` 转为 `input`
  - 单条 user 消息简化为 string
  - 多条消息转为 array
- `max_tokens` → `max_output_tokens`

**Responses → Chat**:
- `input` 转为 `messages`
  - string 转为单条 user 消息
  - array 保持原样
- `max_output_tokens` → `max_tokens`
- 默认 `max_tokens: 4096`

### 4. Anthropic → Gemini (`gemini.rs:7-117`)

**状态**: ⚠️ 基础版，非流式

**字段映射**：

| Anthropic | Gemini | 说明 |
|-----------|--------|------|
| `system` | `systemInstruction.parts[].text` | 系统指令 |
| `messages[].role=user` | `contents[].role=user` | 用户消息 |
| `messages[].role=assistant` | `contents[].role=model` | 助手消息 |
| `content[].type=text` | `parts[].text` | 文本 |
| `content[].type=tool_use` | `parts[].functionCall` | 工具调用 |
| `content[].type=tool_result` | `parts[].functionResponse` | 工具结果 |
| `tools[]` | `tools[].functionDeclarations` | 工具定义 |
| `max_tokens` | `generationConfig.maxOutputTokens` | 最大输出 |
| `temperature` | `generationConfig.temperature` | 温度 |

**限制**：
- 不支持流式响应（Gemini 流式 SSE 格式与 Anthropic 差异大）
- `tool_result` 仅传递字符串 content，不处理复杂结构

### 5. Gemini → Anthropic/OpenAI (server.rs Gemini 端点)

**状态**: ⚠️ 简化版

Gemini 入站请求被转换为极简的 Anthropic 消息：
- 从 Gemini 请求中提取第一个 text part
- 构建为 role=user 的简单消息
- 使用固定 `max_tokens: 4096`
- 实际上丢失了所有 Gemini 特有的功能（多轮对话、功能调用等）

---

## SSE 状态机

### OpenAiToAnthropicStream (`streaming.rs:20-210`)

**状态**:
- `started: bool` — 是否已发射 `message_start`
- `content_block_started: bool` — 是否已发射 `content_block_start`
- `buffer: String` — SSE 数据缓冲区

**状态转移**:
```
初始状态 → [收到首个 content 或 role delta] → message_start
         → [收到首个 text delta] → content_block_start
         → [每个 text delta] → content_block_delta
         → [收到 finish_reason=stop] → content_block_stop → message_stop
         → [收到 finish_reason=tool_calls] → content_block_stop → message_stop
```

### AnthropicToOpenAiStream (`streaming.rs:214-391`)

**状态**:
- `current_tool_name: Option<String>` — 当前正在处理的工具名
- `current_tool_id: Option<String>` — 当前工具调用 ID

**转换逻辑**:
- `message_start` → `data: {"choices": [{"delta": {"role": "assistant"}}]}`
- `content_block_delta.text_delta` → `data: {"choices": [{"delta": {"content": "..."}}]}`
- `content_block_delta.input_json_delta` → `data: {"choices": [{"delta": {"tool_calls": [...]}}]}`
- `message_stop` → `data: {"choices": [{"delta": {}, "finish_reason": "stop"}]}` + `data: [DONE]`
