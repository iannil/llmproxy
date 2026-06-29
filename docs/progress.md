# Progress — llmproxy v0.0.1

> 当前项目进展追踪。对 LLM 大模型友好，便于理解项目状态和执行后续迭代。

---

## Status: ✅ 核心功能已完成

项目已从原 CC Switch Tauri 桌面应用剥离并独立运行。当前处于 **维护期**，核心功能稳定，无阻塞性问题。

## 已完成 ✅

### 核心代理功能
- [x] Axum HTTP 服务器 (7 条路由 + 2 条辅助)
- [x] YAML 配置文件加载 (config.rs)
- [x] 模型路由 (精确匹配 + 前缀匹配)
- [x] 环境变量解析 `${VAR_NAME}`

### 协议转换
- [x] Anthropic ↔ OpenAI Chat 双向转换 (请求 + 流式) — **生产就绪**
- [x] OpenAI Chat ↔ OpenAI Responses 双向转换 (非流式)
- [x] Anthropic → Gemini 基本转换 (非流式)
- [x] Gemini → Anthropic/OpenAI 简化转换 (非流式)

### 安全性
- [x] 模型名注入防护 (拒绝路径穿越字符)
- [x] API Key 环境变量注入
- [x] Gemini 路径校验

### SSE 流式
- [x] OpenAI SSE → Anthropic SSE (`OpenAiToAnthropicStream`)
- [x] Anthropic SSE → OpenAI SSE (`AnthropicToOpenAiStream`)
- [x] Passthrough 模式

## 进行中 🚧

- **文档整理** — 当前正同步更新到 `/docs` 目录
- **项目初始化** — 首次 git 提交尚未完成

## 待办 📋

### 高优先级
- [ ] **首次 Git 提交** — 项目尚未建立 git 历史
- [ ] **基础测试** — 尚无任何单元测试或集成测试

### 中优先级
- [ ] **流式超时处理** — 当前无 SSE 空闲超时机制
- [ ] **更完善的错误传播** — 上游错误有时包含敏感信息
- [ ] **Gemini 流式响应** — 当前仅支持非流式 Gemini 请求

### 低优先级
- [ ] **OpenAI Responses 流式支持** — 当前 Chat ↔ Responses 仅非流式
- [ ] **多模态支持** — image content block 未转换
- [ ] **速率限制** — 无上游并发控制
- [ ] **请求日志格式化** — 当前仅 basic info 级别日志
- [ ] **CI/CD** — 无 GitHub Actions 或其他 CI

## 代码统计

```
╔══════════════════════════╤════════╤═══════════════╗
║ 模块                      │ 行数   │ 说明           ║
╠══════════════════════════╪════════╪═══════════════╣
║ proxy/server.rs     │ 487    │ HTTP 服务器    ║
║ proxy/convert.rs    │ 531    │ 协议转换核心    ║
║ proxy/streaming.rs  │ 391    │ SSE 流式状态机  ║
║ proxy/gemini.rs     │ 184    │ Gemini 转换     ║
║ proxy/forwarder.rs  │ 127    │ 上游请求转发     ║
║ proxy/types.rs      │ 47     │ 类型定义         ║
║ proxy/error.rs      │ 49     │ 错误类型         ║
║ src/config.rs            │ 212    │ 配置管理         ║
║ src/provider.rs          │ 83     │ Provider 结构    ║
║ src/app_config.rs        │ 52     │ AppType 枚举     ║
║ src/error.rs             │ 65     │ 全局错误         ║
║ src/main.rs              │ 83     │ CLI 入口         ║
║ src/lib.rs               │ 7      │ 模块声明         ║
╠══════════════════════════╪════════╪═══════════════╣
║ 总计                      │ ~2,325 │               ║
╚══════════════════════════╧════════╧═══════════════╝
```

## 已知问题

### forwarder.rs 中的未使用的 model_mappings
`resolve_upstream()` 中创建了空的 `HashMap` 然后丢弃 —— 模型映射通过 `ConfigState.get_model_mapping()` 在运行时按需查询。

**影响**: 无功能性影响，仅为死代码。

### gemini.rs 文件名不一致
模块声明为 `pub mod gemini`，但文件名实际为 `gemini.rs`（非 `convert_gemini.rs`，与 AGENTS.md 中描述的 `convert_gemini.rs` 不一致）。

### 上游超时硬编码
`ConfigState::upstream_timeout_secs()` 始终返回 300s，配置文件中无此字段。
