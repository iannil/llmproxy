# llmproxy

> **Unified model API bridge** — route any AI tool to any model provider with seamless protocol conversion.

[中文](README.zh-CN.md)

[![Crates.io](https://img.shields.io/crates/v/llmproxy.svg)](https://crates.io/crates/llmproxy)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
![Rust](https://img.shields.io/badge/rust-1.85.0+-blue.svg)
[![中文](https://img.shields.io/badge/README-中文-blue.svg)](README.zh-CN.md)

```bash
# Claude Code → DeepSeek
export ANTHROPIC_BASE_URL=http://localhost:8880
claude --model deepseek-chat

# Codex → Claude
codex --model claude-sonnet-4-20250514

# Any OpenAI client → Gemini
curl http://localhost:8880/v1/chat/completions \
  -d '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"Hello"}]}'
```

llmproxy sits between your AI CLI tools and upstream model APIs, translating requests and streaming responses on the fly. No database, no sessions — just a lightweight YAML-configured proxy.

---

## Features

- **Multi-protocol**: Anthropic Messages, OpenAI Chat, OpenAI Responses, Google Gemini
- **Bidirectional streaming**: SSE conversion between Anthropic ↔ OpenAI (production-ready)
- **Single YAML config**: no database, no GUI, no complexity
- **Model routing**: exact match + prefix matching across multiple upstreams
- **Model mapping**: rewrite model names per upstream
- **Env-var injection**: `${API_KEY}` references in config
- **Security**: model name injection guard, no authentication layer (transparent proxy)

### Protocol Matrix

| Inbound → Outbound | Anthropic | OpenAI Chat | Responses | Gemini |
|-------------------|-----------|-------------|-----------|--------|
| Anthropic Messages | ✅ pass-through | ✅ + streaming | ✅ | ⚠️ basic |
| OpenAI Chat | ✅ + streaming | ✅ pass-through | ✅ | ⚠️ basic |
| OpenAI Responses | ✅ | ✅ | ✅ pass-through | ⚠️ basic |
| Gemini | ✅ simplified | ✅ simplified | ✅ simplified | ✅ pass-through |

---

## Quick Start

```bash
# 1. Build
cargo build --release

# 2. Configure
cp config.example.yaml config.yaml
# Edit config.yaml with your API keys

# 3. Run
cargo run --release
```

Default listen address: `127.0.0.1:8880` (override with `--listen`).

### Configuration

```yaml
listen: "127.0.0.1:8880"

upstreams:
  - name: deepseek
    base_url: https://api.deepseek.com
    api_key: "${DEEPSEEK_API_KEY}"    # supports env var references
    type: openai                       # anthropic | openai | openai_responses | gemini
    models:
      - deepseek-chat
```

### CLI

```text
Usage: llmproxy [OPTIONS]

Options:
  -c, --config <FILE>  Path to YAML configuration [default: config.yaml]
  -l, --listen <ADDR>  Listen address (overrides config file)
  -h, --help           Print help
  -V, --version        Print version
```

---

## Use Cases

### Claude Code with DeepSeek

```bash
export ANTHROPIC_BASE_URL=http://localhost:8880
export ANTHROPIC_API_KEY=not-needed
claude --model deepseek-chat
```

### Codex / OpenAI SDK with Claude

```python
from openai import OpenAI
client = OpenAI(base_url="http://localhost:8880", api_key="not-needed")
response = client.chat.completions.create(
    model="claude-sonnet-4-20250514",
    messages=[{"role": "user", "content": "Hello!"}]
)
```

### Any CLI with Gemini

```bash
curl -X POST http://localhost:8880/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"gemini-2.5-flash","messages":[{"role":"user","content":"Hello"}],"max_tokens":100}'
```

---

## Routes

| Method | Path | Protocol |
|--------|------|----------|
| POST | `/v1/messages` | Anthropic Messages |
| POST | `/v1/chat/completions` | OpenAI Chat |
| POST | `/v1/responses` | OpenAI Responses |
| GET | `/v1/models` | Model listing |
| ANY | `/v1beta/*` | Google Gemini |
| GET | `/health` | Health check |

---

## Documentation

| Doc | Description |
|-----|-------------|
| [docs/usage-guide.md](docs/usage-guide.md) | Step-by-step tutorial with real scenarios |
| [docs/config-format.md](docs/config-format.md) | YAML configuration reference |
| [docs/api-routes.md](docs/api-routes.md) | Full API reference |
| [docs/conversion-paths.md](docs/conversion-paths.md) | Protocol conversion details |
| [docs/architecture.md](docs/architecture.md) | Architecture & design decisions |

---

## Why llmproxy?

- **~2,300 lines of Rust** — small, auditable, no bloat
- **No external database** — configuration is a single file
- **No state** — restart any time, no session to lose
- **Transparent** — your API keys never leave your network

---

## License

MIT © 2026

---

> *llmproxy — bridge any model, any tool, any protocol.*

