# Roadmap

> llmproxy 未来规划。

---

## Short-term (1-2 周)

### 基础设施
- [ ] **首次 Git 提交** — 建立版本历史
- [ ] **基础单元测试** — 为 convert.rs 和 streaming.rs 添加核心转换逻辑测试
- [ ] **集成测试** — 使用 mock 服务器测试路由转发

### 质量改进
- [ ] **删除 forwarder.rs 死代码** — `resolve_upstream()` 中未使用的 `model_mappings`
- [ ] **修复 AGENTS.md 文件名描述** — `gemini.rs` 而非 `convert_gemini.rs`
- [ ] **配置文件超时可配置** — `upstream_timeout_secs` 从配置读取

## Medium-term (1-2 月)

### 功能增强
- [ ] **Gemini 流式支持** — 实现 Gemini SSE ↔ Anthropic SSE 转换
- [ ] **OpenAI Responses 流式支持** — 实现 Responses SSE 格式转换
- [ ] **多模态内容** — 支持 image/audio content block 转换
- [ ] **上游健康检查** — 定期检测上游可用性

### 运维
- [ ] **Prometheus 指标** — 请求计数、延迟、错误率
- [ ] **结构化日志** — 统一 JSON 日志格式
- [ ] **Dockerfile** — 容器化部署

## Long-term (3 月+)

- [ ] **配置热重载信号** — SIGHUP 触发配置重载
- [ ] **上游自动重试** — 可配置的指数退避重试
- [ ] **速率限制** — 可配置的每上游 QPS 限制
- [ ] **请求/响应缓存** — 相同请求的可选缓存
