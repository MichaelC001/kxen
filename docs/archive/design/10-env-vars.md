# 环境变量约定

- 日期: 2026-07-20
- 原则: 沿用 pi 的惯例，kxen 只加 kxen 特有的；配置传递走参数不走 env 侧信道

## 1. kxen 自有变量

| 变量 | 语义 | 默认 |
| --- | --- | --- |
| `KXEN_AGENT_DIR` | kxen agent 目录（auth.json / sessions / prompts / 内存 / dumps） | `~/.kxen/agent` |
| `KXEN_VERSION` | 版本号（编译期 `--define` 注入，源码直跑回落 0.0.0） | `0.0.0` |

## 2. 穿透给 pi 的变量（pi 原生读取，kxen 不拦截）

| 变量 | 语义 |
| --- | --- |
| `PI_CODING_AGENT_DIR` | pi 的 agent 目录重定位；kxen 启动时设为 `KXEN_AGENT_DIR` 的值 |
| `HTTP_PROXY` / `HTTPS_PROXY` | pi 的出站代理 |
| `PI_OFFLINE` | pi 离线模式 |
| `PI_TELEMETRY` | pi 遥测开关 |
| `PI_SKIP_VERSION_CHECK` | pi 版本检查跳过 |

## 3. Provider 凭证变量（行业标准名）

| 变量 | 对应 provider |
| --- | --- |
| `ANTHROPIC_API_KEY` | anthropic（订阅 OAuth 优先，此为兜底） |
| `OPENAI_API_KEY` | openai-codex（ChatGPT OAuth 优先） |
| `XAI_API_KEY` | xai |
| `MOONSHOT_API_KEY` / `KIMI_API_KEY` | kimi-coding |

凭证解析顺序（providers 包）：kxen auth.json -> 官方 CLI 现有凭证（~/.claude、~/.codex/auth.json、~/.grok/auth.json、~/.kimi-code/credentials）-> 环境变量。OAuth 一律以官方 CLI 的新鲜凭证为准（防轮换过期）。

## 4. 反模式（已踩过，禁止回潮）

- 不用 env 做进程内配置传递（flag -> env -> 下游读取）：一律直接传参
- 不私造 `KXEN_PLAN` / `KXEN_YOLO` / `KXEN_MODEL` 这类旁路变量：模型与模式走 pi 原生 `--model` 与会话内 slash 命令
- 不拦截 pi 的 `PI_*` 变量做二次解释
