# 四大订阅认证复用调研

- 调研日期: 2026-07-20
- 结论: 四个订阅全部存在可行的第三方接入路径；Claude 是唯一有明确 ToS 风险的一个

## 1. 总览

| 订阅 | 可行路径 | 官方态度 | 风险等级 |
| --- | --- | --- | --- |
| Claude Pro / Max | OAuth PKCE 自发 flow，或复用 Claude Code 本地凭证 | Anthropic 明确禁止官方客户端外使用订阅 token | 高 |
| Codex (ChatGPT Plus / Pro) | ChatGPT OAuth（OpenCode 官方零配置支持） | 默许（OpenAI 未禁止第三方 OAuth 接入） | 低 |
| Grok (SuperGrok / X Premium+) | xAI 官方宣布支持 OpenCode 接入；复用 grok-cli 公开 OAuth client | 官方支持 | 低 |
| Kimi (Kimi Code 会员) | 订阅后 Console 发放 API Key，走会员配额；OpenAI / Anthropic 双协议 | 官方支持 | 低 |

## 2. Claude (Anthropic Pro / Max)

已验证事实：

- OpenCode 官方 providers 页提供「Claude Pro/Max」OAuth 登录选项；同页注明「There are plugins that allow you to use your Claude Pro/Max models with OpenCode. Anthropic explicitly prohibits this.」（来源: https://opencode.ai/docs/providers/ ）
- Pi 内置「OAuth authentication for Claude Pro/Max subscriptions」（来源: https://mariozechner.at/posts/2025-11-30-pi-coding-agent/ ）
- 社区插件直接复用 Claude Code 本地凭证：macOS Keychain 的 `Claude Code-credentials*` 条目，其他平台读 `~/.claude/.credentials.json`，自动刷新（来源: https://github.com/griffinmartin/opencode-claude-auth ）
- 另有 patch 类方案伪装 `user-agent: claude-cli/...`、注入必需 beta header（`oauth-2025-04-20`、`interleaved-thinking-2025-05-14`）、给 `/v1/messages` 加 `?beta=true`（来源: https://github.com/micuintus/OpenCode-Claude-Auth-03-2026 ）

kxen 决策：

- 提供两种模式：自发 OAuth PKCE flow（登录用户自己的 Claude 账号）与读取本机 Claude Code 凭证（零配置）
- 首次启用时明确展示 ToS 风险提示，让用户自行选择；文档与代码注释中不做隐瞒
- 配额感知：无官方接口，只能靠 429 / rate limit header 被动感知

## 3. Codex (ChatGPT Plus / Pro)

已验证事实（来源: https://developers.openai.com/codex/auth 与 https://openai-codex.mintlify.app/cli/login ）：

- `codex login` 走浏览器 OAuth，本地起 `http://localhost:1455` 回调；凭证缓存于 `~/.codex/auth.json`，CLI 与 IDE 扩展共享
- token 自动刷新；headless 环境用 `codex login --device-auth`
- `forced_login_method = "chatgpt"` 可强制登录方式
- OpenCode 官方支持「ChatGPT Plus/Pro」登录，与 GitHub Copilot、GitLab Duo 并列为「zero setup」订阅（来源: https://opencode.ai/docs/providers/ ）

kxen 决策：

- 优先自发 OAuth flow（同 OpenCode 的实现路径）；零配置场景可直接读 `~/.codex/auth.json` 复用 Codex CLI 已登录凭证
- 配额感知：无官方接口，靠限流信号被动感知

## 4. Grok (SuperGrok / X Premium+)

已验证事实：

- xAI 官方新闻「Use Grok in OpenCode」(2026-05-21)：OpenCode 内 `/connect` 选 xAI，提供「xAI Grok OAuth (SuperGrok Subscription)」浏览器流程与「Headless / Remote / VPS」device-code 流程（来源: https://x.ai/news/grok-opencode ）
- OpenCode 实现复用 grok-cli 的公开 desktop OAuth client_id（`b1a00492-...`），授权端点 `https://auth.x.ai/oauth2/authorize`，scope 含 `grok-cli:access` 与 `api:access`，device code 走 RFC 8628（来源: https://github.com/anomalyco/opencode/commit/b32debb8a3327a6cf2b9b9face7f296acc5a1458 ）
- grok-build 自身的认证链：per-model api_key > `~/.grok/auth.json` 会话 token > `XAI_API_KEY`（来源: https://github.com/xai-org/grok-build 用户指南）
- 已知坑：xAI 后端对 OAuth API 面有 allowlist，部分 SuperGrok 档位登录成功但推理返回 403，需要切回 API key 路径（来源: https://hermes-agent.nousresearch.com/docs/guides/xai-grok-oauth ，issue #26847）

kxen 决策：

- 实现与 OpenCode 相同的 OAuth + device code 双流程；也支持读取 `~/.grok/auth.json` 复用
- 403 个案按「降级到 XAI_API_KEY 或 fallback 角色模型」处理，纳入资源管理器的 provider 健康度

## 5. Kimi (Kimi Code 会员)

已验证事实（来源: https://www.kimi.com/code/docs/en/ 与 membership 页）：

- 会员档位: Moderato $19 / Allegretto $39 / Allegro $99 / Vivace $199（月付），Kimi Code credits 随档位 1x / 5x / 15x / 30x
- 官方明确支持第三方工具：订阅者在 Kimi Code Console 获取 API Key，请求计入会员配额
- API 双协议：OpenAI 兼容 `https://api.kimi.com/coding/v1`，Anthropic 兼容 `https://api.kimi.com/coding/`
- 模型: `k3`（Moderato+，Allegretto+ 解锁 1M 上下文）、`kimi-for-coding`（K2.7 Code，全档位）、`kimi-for-coding-highspeed`（Allegretto+）
- CLI 内 `/login` OAuth、`/usage` 查配额；超额可开 Extra Usage 按量补
- Pi 生态已有 `pi-provider-kimi-code` 参考实现

kxen 决策：

- 默认走官方 API Key 路径（OpenAI / Anthropic 双协议任选，建议 Anthropic 兼容以获得更好的 thinking 支持，待实测）
- `/usage` 类接口是四个订阅里唯一明确的配额探测点，优先接入资源管理器
- 可选支持 OAuth 复用 kimi CLI 凭证：`~/.kimi-code/credentials/kimi-code.json`（access_token / refresh_token / expires_at，2026-07-20 实测存在并成功调用 `/coding/v1/models`）

## 6. 对资源管理器的要求汇总

- Claude / Codex / Grok 配额：只能被动感知（429、rate limit header、错误文案），资源管理器按「信号驱动」降并发、退避、切 fallback
- Kimi 配额：可主动探测，支持预算预分配
- 所有凭证统一进加密 auth store，禁止写进仓库与 `.env`；多账号场景参考 OMP 的 round-robin 轮换
