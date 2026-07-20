# 角色化模型路由与全局资源管理

- 日期: 2026-07-20
- 状态: 初稿，待评审
- 前置阅读: `docs/research/02-pi-and-omp.md`（OMP 路由机制）、`docs/research/03-subscription-auth.md`（配额可探测性）

## 1. 为什么约束必须是全局的

多订阅混用时没有任何一家有统一 API 限额视图：Claude / Codex / Grok 只能被动感知限流，Kimi 可以主动查配额。如果约束只挂在某个功能（如 workflow）上，直接 sub-agent、goal 并行、review agent 就会绕过限制把订阅打爆。因此约束作用于模型调用本身，与功能无关。

## 2. 角色模型（Role Router）

内置角色（可配置扩展）：

| 角色 | 用途 | 典型绑定（示例） |
| --- | --- | --- |
| thinking | 深度推理、架构决策 | Claude Opus 档 / slow |
| planning | 目标拆解、workflow 脚本生成 | Claude Sonnet 档 |
| execution | 大批量重复执行 | Grok / kimi-for-coding-highspeed |
| review | 对抗审查、验证 | Claude Sonnet 档（与 planning 不同账号池可轮换） |
| research | 搜索、阅读、摘要 | Kimi / 便宜模型 |
| tiny | 标题、记忆、轻量分类 | 最便宜可用模型 |

规则：

- 角色 -> `provider/model` + thinking level + fallback 链（参考 OMP `modelRoles` + `retry.fallbackChains`，支持 `provider/*` 通配与 `cooldown-expiry` 恢复）
- subagent / workflow 阶段 / goal 执行都按角色申请模型，不直接写死 provider
- 同 provider 多凭证 round-robin（OMP 已验证该模式），session 亲和

## 3. Model Resource Manager（MRM）

全局单例，所有模型调用的唯一入口。

### 约束维度

| 维度 | 说明 | 数据来源 |
| --- | --- | --- |
| provider 并发 | 每 provider 同时运行的调用数 | 配置 + 限流信号动态下调 |
| 角色并发 | 每角色同时运行的调用数 | 配置 |
| 全局并发 | 进程内总调用数上限 | 配置（默认参考机器核数） |
| 速率 | RPM / TPM 窗口 | 配置初始值 + 429 / rate-limit header 自适应 |
| 配额 | 订阅剩余额度 | Kimi 主动探测；其余被动感知 |
| 预算 | 会话级 / goal 级 token 与成本 | pi-ai 的 usage 统计累加 |

### 生命周期

```
acquire({ role, estimatedTokens, priority }) -> 选 provider（首选 -> fallback）
  -> 检查并发 / 速率 / 配额 / 预算 -> 通过则发 slot，否则排队
use(slot)   -> 执行调用，记录 usage
release(slot) -> 归还并发位，结算预算，更新 provider 健康度
```

### 降级行为

- 429 / 配额墙 / 超时：该 provider 进入冷却，同角色 fallback 链取下一个
- 冷却恢复策略：`cooldown-expiry`（OMP 同款）
- 预算接近耗尽：提前向编排层发警告（80% / 95% 两档），goal / workflow 可主动收敛规模
- 所有降级事件进入事件流，TUI 可见

### 对 AI 的可见性（核心差异点）

让做规划 / 写 workflow 的模型能感知余量：

- planning / thinking 角色的系统上下文里注入当前状态快照（每 provider 并发占用、冷却中、Kimi 剩余配额、预算水位）
- workflow 脚本提供 `constraints()` API 返回同一份快照
- 推荐策略以自然语言给出（如「execution 优先走 Grok，Claude 留给 thinking / review」），由模板根据状态生成

## 4. 配置示例（TOML；kxen 配置文件一律 TOML，2026-07-20 决策）

```toml
[roles.thinking]
model = "anthropic/claude-opus-4-5:high"
fallbacks = ["openai/gpt-5.5:high"]

[roles.planning]
model = "anthropic/claude-sonnet-4-5"
fallbacks = ["kimi/k3"]

[roles.execution]
model = "xai/grok-4.5"
fallbacks = ["kimi/kimi-for-coding-highspeed", "openai/gpt-5.5-codex"]

[roles.review]
model = "anthropic/claude-sonnet-4-5"
fallbacks = ["kimi/k3"]

[roles.research]
model = "kimi/kimi-for-coding"
fallbacks = ["openai/gpt-5.5-mini"]

[roles.tiny]
model = "openai/gpt-5.5-mini"

[limits.global]
concurrent = 16

[limits.providers.anthropic]
concurrent = 4
rpm = 50

[limits.providers.openai]
concurrent = 4
rpm = 50

[limits.providers.xai]
concurrent = 8
rpm = 60

[limits.providers.kimi]
concurrent = 5
rpm = 60

[limits.roles.execution]
concurrent = 8

[limits.roles.thinking]
concurrent = 2

[budgets.session]
tokens = 2000000

[budgets.perGoal]
tokens = 800000
agents = 40
```

## 5. 与 OMP / Claude 的对应关系

| 能力 | OMP | Claude Code | kxen |
| --- | --- | --- | --- |
| 角色路由 | modelRoles（10 个内置角色） | 无（仅 SUBAGENT_MODEL 整体覆盖） | 角色 + thinking level + fallback |
| 降级 | retry.fallbackChains | 无 | 同 OMP 思路，挂 MRM 健康度 |
| 并发 | subagent 并行上限 | 固定 16 / 1000 | 全局 + provider + 角色三层动态 |
| 配额 | 无（靠 429 退避） | 无 | Kimi 主动探测 + 其余被动感知 |
| 预算 | 无 | 大 run 警告（只提示） | 预算账户 + 水位警告 + 硬停 |
