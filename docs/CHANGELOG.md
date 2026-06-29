# Changelog

> llmproxy 版本变更历史。

---

## [0.0.1] — 当前版本

### 新增
- Anthropic ↔ OpenAI Chat 流式双向转换（生产就绪）
- OpenAI Chat ↔ Responses 协议转换（非流式）
- Anthropic → Gemini 基本转换
- Gemini → Anthropic/OpenAI 简化转换
- YAML 配置文件 + 环境变量引用
- 模型名注入防护

### 技术栈
- Rust 2021 edition (MSRV 1.85.0)
- tokio + axum HTTP 服务器
- reqwest HTTP 客户端（连接池 32）

---

> 此项目从原 CC Switch Tauri 桌面应用剥离。剥离前的版本历史不在此记录。
