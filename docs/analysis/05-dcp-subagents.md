# 分析: DCP (Deterministic Context Pipeline) 对 kxen 子代理的适用性

- 日期: 2026-07-20
- 调研对象: https://github.com/Menci/Cahciua （Memoh 研究性附属项目，Telegram 群聊 bot）
- 依据: README、AGENTS.md、https://github.com/Menci/Cahciua/blob/main/docs/dcp-design.md
- 结论先行: 有效，且集中在五点：provider 中立的 IR 存储、请求时确定性 compose 规则、probe gate 降本、epoch cursor 压缩 + replay/resume、注入防护。但它解决的是「上下文构造」，不解决执行隔离与工具不变量，两块互补

## 1. DCP 是什么

核心主张: 不把 LLM transcript 当作权威状态，而是存「平台事件 + turn 响应」，每次请求时用纯函数流水线确定性重建上下文。

四层（严格单向，所有权清晰）：

1. Adaptation: 平台事件 -> 规范化事件（CanonicalIMEvent），双时间戳（本地排序时间 + 给模型看的服务端时间）
2. Projection: 纯函数 reducer `IC' = reduce(IC, event)`，无 I/O，永不接触 LLM 输出
3. Rendering: IC -> 分段的、provider 无关的 XML 渲染上下文（RC）
4. Driver: 把 RC 与已存的 Turn Responses（TR）按时间戳归并，触发 LLM 调用；TR 以 provider 中立的 IR（`ConversationEntry[]`）持久化，而不是各家 wire 格式

「Deterministic」的含义: 不维护上下文，维护上下文的构造过程。冷启动重放与实时处理产生相同的上下文序列，任何一段都可单测、可在备好数据集上评测迭代。

## 2. 对子代理有直接价值的五个机制

### 2.1 TR 以 provider 中立 IR 持久化（多模型混用的关键基础设施）

kxen 的子代理会在 fallback 链上换模型（Claude 限流 -> Grok）。Cahciua 的做法是：历史只存 IR，wire 差异全部推到「请求时的 codec 边界」处理；模型不兼容时剥离 reasoning、工具 call id 只在 wire 层清洗。这正是多模型 sub-agent 中途换模型不炸历史的正规做法。pi-ai 做了调用层归一，DCP 把它形式化到了存储层。

### 2.2 composeContext() 请求时确定性优化规则（可直接照抄的规则集）

每次组装请求时按固定规则瘦身：

- 只保留最新 5 轮纯文本 assistant 输出
- 去重自己已发出消息的 RC 副本（发送工具调用已代表它们）
- 降低旧图片 detail、裁旧超大工具结果
- 模型身份不兼容时 sanitize reasoning
- 工具 id 在 wire 边界清洗
- 前置最新压缩摘要

确定性 = 同样的输入永远得到同样的请求，这对 workflow resume（重放缓存结果）和调试（这个 sub-agent 当时到底看到了什么）价值极大。

### 2.3 Probe gate（fan-out 降本，特定功能子代理最受益）

每次唤醒先跑一个「外部裁判」probe：只给一个强制工具 `decide(should_act, reason)`，输出永不进入主上下文，缺省/畸形一律 fail closed；主循环只有 probe 放行才激活。

映射到 kxen：workflow 里审计类 fan-out（几百个文件各派一个执行 agent）前，用 tiny 角色模型先 probe 一遍「这个文件值得审吗」，可省掉大量无效执行。review / research 这类特定功能子代理同理：probe 决定激活与否与输入裁剪。这是 DCP 里对「特定功能子代理」最直接可用的一条。

### 2.4 Epoch cursor 压缩 + 追加式摘要（与 replay 兼容的压缩）

压缩是独立控制器：高水位（maxContextEstTokens）触发，低水位（workingWindowEstTokens）选新 cursor；摘要是 append-only 行，更新 cursor 即生效；历史事件与 TR 永不删除；摘要不计入触发估算。

对比各家「重写历史」的压缩，这个设计与 DCP 的 replay 天然兼容（压缩本身也是可重放的状态推进），也天然 KV-cache 友好（前缀稳定）。kxen 的 workflow 要求「同会话 resume 时已完成的 agent 直接回放」，配这个模型最干净。

### 2.5 XML fencing 与身份编码（处理不可信内容的子代理）

用户身份只进 XML 属性、内容转义、无法注入兄弟消息属性；被 block 的来源在渲染时掩码而不是删事件（配置变了重放可恢复）。research 类子代理要读网页 / issue / 外部文本，这套注入防护直接可用。

## 3. 有参考价值但需改造的部分

- 调度器（alien-signals，发送者感知 debounce、调用串行化、步骤持久化后才检查中断、协作式中断不抢占）：群聊场景的发抖抑制对 kxen 意义不大，但「完成步骤先持久化、再在步骤边界检查中断」是 crash-safe resume 的好规矩，可进 workflow runtime
- 合成自事件（自己的动作立即变成 canonical 事件，下一个 probe 可见）：对应 kxen 多 agent 通信时的因果一致性，做 channel / 协作时再引入
- 强制工具选择的 runner 重试（provider 忽略 forced choice 时重试至多 3 次、累计 usage、只执行最终选中输出）：probe gate 落地的配套细节，多 provider 下必需

## 4. 不适用 / 边界

- DCP 解决上下文构造，不解决执行：worktree 隔离、edit staleness、权限规则仍是 kxen 自己的工具面问题（见 analysis/02）
- 确定性有边界：Cahciua 的输入是已持久化的 IM 事件；kxen 的工具结果来自活文件系统，replay 只能保证「给定已存输入则上下文一致」，不能跨文件系统变化做 bit 级重放。检查点（shadow git 快照）是补齐这个缺口的手段
- 事件溯源 + IC/RC 双层驻留有存储与复杂度成本：kxen 应只在 sub-agent / workflow 范围内引入，主会话维持现状

## 5. 落地映射（kxen 模块 -> DCP 机制）

| kxen 模块 | 引入 | 优先级 |
| --- | --- | --- |
| subagent 会话存储 | TR 以 provider 中立 IR 持久化，wire codec 边界清洗 | P0（多模型 fallback 的根基） |
| subagent 上下文组装 | composeContext() 规则集（2.2 六条） | P0 |
| workflow resume | epoch cursor 压缩 + 确定性重建，回放缓存结果 | P1（M4） |
| 特定功能子代理（review / research / audit） | probe gate: tiny 模型强制 `decide` 工具，fail closed，probe 输出不进主上下文 | P1（M2/M4 降本） |
| workflow runtime | 步骤持久化后才检查中断；forced choice 重试与 usage 累计 | P1 |
| research 子代理 | XML fencing + 渲染时掩码 | P2 |
| eval 管线（T12） | 利用确定性重建做数据集级 context 策略评测 | P2（M5） |

## 6. 源码级精确参数（2026-07-20 直读 src/driver/context.ts 等）

composeContext() 的实际常量与顺序：

- token 估算: 中英文混合按 2 字符/token；图片统一按 100 token（Claude 缩略图公式）
- `KEEP_NO_TOOL_CALL_TRS = 5`: 无工具调用的纯文本 assistant 轮只留最新 5 轮
- `TOOL_RESULT_TRIM_THRESHOLD = 512`、`TOOL_RESULT_KEEP_RECENT_OVERSIZED = 5`: 超大工具结果只留最新 5 个原样，其余截断为「头 200 + 尾 200 + 省略注记」；旧图片一律降为 detail=low；截断处做 UTF-16 surrogate 安全处理
- 顺序: 过滤 isSelfSent 段 -> 剥离非同模型 TR 的 reasoning（签名只在同模型内 round-trip）-> 裁纯文本轮 -> 裁工具结果 -> 按时间戳 merge -> 前置 `[Conversation summary]` -> 删除剥离 reasoning 后变空的 assistant 消息 -> trimEntries 到预算（从头丢，且绝不以孤儿 toolResult 开头：带 toolCall 的 assistant 被丢时其后续 toolResult 一并丢）
- merge: RC 与 TR 按时间戳归并，同刻 RC 在前（满足 Anthropic 角色交替）；连续 RC 段合并为一条 user message
- probe 上下文是另一套组装: 不进真实 TR，而是把每个工具调用合成 `<tool-call name t><args>CDATA</args><result>CDATA</result></tool-call>`（参数与结果各截到 1024，0.4/0.4 头尾比），`send_message` 跳过（已由 bot 自己的消息代表）；CDATA 用 `]]]]><![CDATA[>` 转义，属性用 XML 转义
- late-binding prompt 以合成 user message 追加在请求末尾，不进 IC

compaction-controller（alien-signals effect 驱动）：

- 监听 context 信号 -> debounce 检查 -> 超出 `maxContextEstTokens` 才触发 -> `findWorkingWindowCursor` 按 `workingWindowEstTokens` 从新到旧回走选 cursor -> 只对 `[oldCursor, newCursor)` 窗口生成摘要 -> 持久化后更新信号，cursor effect 把新 cursor 应用回 Pipeline
- 摘要模型缺省回落到 primary 模型；running 期间的变更置 pendingRecheck 完成后重查

## 7. 一句话总结

DCP 的价值不在「又一个 agent 框架」，而在把「子代理上下文」变成可重放、可评测、可多模型切换的纯函数产物。kxen 采纳其 IR 存储、compose 规则、probe gate、cursor 压缩四点即可拿到 80% 收益，且与已定的 OMP / Claude 机制（snapcompact、clearing、workflow resume）不冲突，是互补的底层形式化。
