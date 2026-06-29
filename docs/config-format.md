# Configuration Format

> 详细介绍 llmproxy 的 YAML 配置文件格式。

---

## 文件位置

默认从工作目录的 `config.yaml` 加载，可通过 `--config` 参数指定。

## 顶级结构

```yaml
# 代理监听地址（默认 127.0.0.1:8880）
listen: "127.0.0.1:8880"

# 上游供应商列表
upstreams:
  - name: <供应商名称>
    base_url: <API 基础 URL>
    api_key: <API 密钥>
    type: <出站协议类型>
    models:
      - <模型名>
    model_mappings:    # 可选
      <入站模型名>: <上游模型名>
    headers:           # 可选
      <Header名>: <值>
```

## 字段说明

### `listen`
- 类型: `string`
- 默认: `"127.0.0.1:8880"`
- 可通过 CLI 参数 `--listen` 覆盖

### `upstreams[]`

每个上游供应商的配置。

#### `name`
- 类型: `string`
- 唯一标识，用于日志和路由

#### `base_url`
- 类型: `string`
- 上游 API 基础 URL，如 `https://api.deepseek.com`

#### `api_key`
- 类型: `string`
- 支持环境变量引用：`"${DEEPSEEK_API_KEY}"`
- 运行时自动替换为实际环境变量值

#### `type`
- 类型: `enum`
- 可选值:
  - `anthropic` — Anthropic Messages API
  - `openai` — OpenAI Chat Completions API（默认）
  - `openai_responses` — OpenAI Responses API
  - `gemini` — Google Gemini API
- 默认: `openai`

#### `models`
- 类型: `string[]`
- 此上游可处理的模型名列表
- 支持前缀匹配（`claude-*` 匹配 `claude-sonnet-4-20250514`）

#### `model_mappings`
- 类型: `map[string]string`
- 可选
- 入站模型名 → 上游实际模型名的映射
- 如不配置，则原样透传 model 字段

#### `headers`
- 类型: `map[string]string`
- 可选
- 自定义请求头，附加到转发请求中

## 完整示例

```yaml
listen: "127.0.0.1:8880"

upstreams:
  # OpenAI Chat 兼容上游
  - name: deepseek
    base_url: https://api.deepseek.com
    api_key: "${DEEPSEEK_API_KEY}"
    type: openai
    models:
      - deepseek-chat
      - deepseek-reasoner

  # Anthropic 官方 API
  - name: claude-official
    base_url: https://api.anthropic.com
    api_key: "${ANTHROPIC_API_KEY}"
    type: anthropic
    models:
      - claude-sonnet-4-20250514
      - claude-opus-4-20250514
    model_mappings:
      claude-sonnet-4-20250514: claude-sonnet-4-20250514

  # Google Gemini
  - name: gemini
    base_url: https://generativelanguage.googleapis.com
    api_key: "${GEMINI_API_KEY}"
    type: gemini
    models:
      - gemini-2.5-flash
      - gemini-2.5-pro
```

## 使用场景

### Claude Code 使用 DeepSeek

```sh
export ANTHROPIC_BASE_URL=http://127.0.0.1:8880
export ANTHROPIC_API_KEY=not-needed
claude --model deepseek-chat
```
入站请求格式: Anthropic Messages → 转换为 OpenAI Chat → 转发到 DeepSeek

### Codex 使用 Claude 官方

```sh
codex --model claude-sonnet-4-20250514
# 配置 Codex 的 base_url = http://127.0.0.1:8880
```
入站请求格式: OpenAI Chat → 转换为 Anthropic Messages → 转发到 Anthropic 官方

## 环境变量解析

`api_key` 字段支持 `${VAR_NAME}` 语法：

```yaml
api_key: "${DEEPSEEK_API_KEY}"
```

解析规则：
1. 加载配置时，扫描所有 `${...}` 模式
2. 从进程环境变量中查找对应名称
3. 替换为实际值（如未设置，保留原样）

## 模型路由规则

模型路由按以下顺序匹配：
1. **精确匹配**：模型名完全匹配 `models` 列表中的某项
2. **前缀匹配**：模型名以某模式开头，或模式以模型名开头
