# 分析: 多提供商模型调度

- 日期: 2026-07-20
- 方法: exa 实搜 + 各 provider 官方限流文档 + Promptfoo / OMP 等实现
- 结论: 调度的本质是「信号驱动的供给管理」。kxen MRM 定稿为: token bucket 预检 + AIMD 自适应并发 + 优先级队列 + 熔断 + 角色 fallback 链，全部信号对编排 AI 可见

## 1. 各提供商的限流信号（已核实）

| Provider   | 维度                         | 信号                                                                                                          | 来源                                |
| ---------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| OpenAI     | RPM + TPM（项目级另算）      | `x-ratelimit-limit/remaining/reset-requests                                                                   | tokens`，429 带 `retry-after`       | https://developers.openai.com/api/docs/guides/rate-limits |
| Anthropic  | RPM + TPM + 并发请求三维     | `anthropic-ratelimit-requests-*`、`anthropic-ratelimit-tokens-*`（每个响应都带）；`retry-after` 只在 429 出现 | sitepoint 生产指南 + Anthropic 文档 |
| xAI (Grok) | 订阅 OAuth 有 allowlist 个案 | 403 需切 API key 或 fallback；其余按 OpenAI 兼容处理                                                          | hermes 文档 issue #26847            |
| Kimi       | 会员配额（月度池）+ 速率     | `/usage` 可主动查余额；OpenAI / Anthropic 双协议                                                              | https://www.kimi.com/code/docs/en/  |

结论: 信号质量分两档。Kimi 可主动探测做预算预分配；Claude / Codex / Grok 只能靠响应头与 429 被动感知，调度器必须「信号驱动」而不是「配置驱动」。

## 2. 机制库（各实现中验证过的）

| 机制              | 细节                                                                                             | 来源                        |
| ----------------- | ------------------------------------------------------------------------------------------------ | --------------------------- |
| token bucket 预检 | 按账户真实 RPM / TPM 的 80-90% 设桶，调用前扣估算 token，桶空则本地等待；把硬 429 变成可控软延迟 | Boundev 429 生产指南        |
| 三级等待          | `retry-after` 优先 -> reset 头换算 -> 全抖动指数退避（兜底），cap 重试次数                       | sitepoint / KissAPI         |
| AIMD 自适应并发   | 429 -> 并发减半；持续成功 -> +1；remaining < 10% -> 主动降并发；每 provider 独立追踪             | Promptfoo 调度器            |
| 三级状态机        | normal (16) / warning (remaining <20% -> 8) / critical (<10% -> 3 且只跑高优先级)                | KissAPI                     |
| 熔断器            | closed -> 连续失败 open -> 冷却后半开探针；防止持续无效重试耗尽线程与配额                        | sitepoint                   |
| 有界队列 + 优先级 | 并发上限使延迟有界；队列深度超阈值时对低优先级 fail fast；交互式请求插队                         | Boundev                     |
| 多凭证轮询        | 同 provider 多 key 轮换，session 亲和 + 按凭证退避                                               | OMP round-robin credentials |
| fallback 链       | 按角色 / 精确模型 / `provider/*` 通配；429 或配额墙触发切换，`cooldown-expiry` 后回主模型        | OMP retry.fallbackChains    |
| context promotion | 上下文溢出先升档到大上下文兄弟模型再考虑压缩                                                     | OMP                         |
| 池化兜底          | 单账户到顶时跨 key / 跨 provider 池化                                                            | Boundev（列为最后手段）     |

## 3. 订阅制下的特殊性

- 订阅配额是「日 / 周 / 月池」，不是纯速率：短时不过热也可能把月度池烧穿，因此预算账户（token / 成本）与速率限制同等重要
- OAuth 订阅通道的用量往往没有 API 返回（Anthropic 订阅不暴露剩余量），只能以「自己的 usage 统计 + 429 信号」建模
- 各订阅的速率曲线不同：execution 类高并发角色应默认绑定限额更宽的订阅（Grok），thinking / review 类低并发高价值角色绑定 Claude

## 4. kxen MRM 定稿算法

调用路径（唯一入口，无旁路）：

```
acquire(role, estimatedTokens, priority):
  1. 角色路由: 取 role 的候选链（首选 -> fallback）
  2. 健康过滤: 剔除熔断中 / 冷却中 / 预算不足的 provider
  3. 信号预检: 首选 provider 的 token bucket（80-90% 口径）够则发 slot，不够则:
     a. 同链取下一个健康 provider
     b. 全链受限 -> 进优先级队列等待（高优先级先出；队列溢出则 fail fast）
  4. 并发闸门: 全局 + provider + 角色三层信号量，AIMD 动态调整各层上限
use(slot): 执行调用，流式统计 usage
release(slot, result):
  5. 结算: token / 成本记入会话与 goal 预算账户
  6. 信号回写: 响应头 remaining -> 更新 bucket 与状态机档位；429 -> AIMD 减半 + provider 冷却；403(OAuth) -> 标记该通道降级并建议切 key / fallback
```

降级链：

- 单点失败: 重试（三级等待，cap 次数，仅幂等）
- 持续失败: 熔断 -> 同角色 fallback
- 恢复: `cooldown-expiry` 回主模型（OMP 策略）
- 上下文溢出: 先 context promotion，再压缩（OMP 策略）

对编排 AI 的可见性（与 `design/02` 对齐）：

- 快照内容: 各 provider 并发占用 / bucket 水位 / 状态机档位 / 冷却倒计时 / Kimi 余额 / 预算水位
- 注入点: planning 与 thinking 角色的系统上下文（每轮刷新）+ workflow 脚本 `constraints()` API
- 推荐语模板化生成（如「execution 优先 Grok，Claude 保留给 thinking / review」），AI 只做选择不做猜测

## 5. 与既有方案的分工

- pi-ai 负责: 单调用层面的协议适配、流式、usage 统计
- MRM 负责: 供给管理（并发 / 速率 / 配额 / 预算 / 健康度）
- 角色路由负责: 需求表达（什么任务该用什么能力档的模型）
- 三者正交，任何一层可独立替换（例如把 pi-ai 换成自研 transport 不影响 MRM）
