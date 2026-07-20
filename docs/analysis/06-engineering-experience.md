# 分析: 工程化体验（内存、性能、mermaid 渲染、可观测性）

- 日期: 2026-07-20
- 方法: exa 实搜 + 公开 issue 事故报告 + grok-build 仓库直读
- 结论: 内存是 TS 系 harness 的第一工程事故源，必须有进程级内存预算；mermaid 走纯 Rust 渲染 + kitty 协议，与我们的 N-API 路线一致；可观测性（telemetry + dump）从第一天内置

## 1. 内存：两个头部产品的公开事故（反面教材）

### Claude Code 的内存事故群（GitHub issues，2026 年）

| 根因 | 细节 | 来源 |
| --- | --- | --- |
| UI 消息数组无界 | `mutableMessages` 只增不减；autocompact 只裁发给 API 的消息，不裁 display/transcript/file-snapshot 状态 | issue #25926 |
| 工具结果驻留阈值过高 | 仅 >400KB 才落盘，50 个 300KB 结果就是 15MB 常驻 | 同上 |
| SDK 双数组 | `messages[]` + `receivedMessages[]` 并行保留 | 同上 |
| 流未排空 | fetch `Response` body 未完全消费 / 未 cancel，ArrayBuffer 以约 205MB/h 累积（8 小时 27.6GB） | issue #33380 |
| 无总预算 | 单项有 cap，进程无聚合上限；V8 与 JSC 构建同样泄漏 -> 应用层保留问题，与运行时无关 | issues #56693 / #56960 |

### OpenCode 的内存事故群

| 根因 | 细节 | 来源 |
| --- | --- | --- |
| 事件队列无背压 | SSE `AsyncQueue` 无容量上限，慢客户端下 `session.diff`（含完整前后文件内容）无限堆积，实测 187GB RSS | issue #16697 调查报告 |
| 订阅不失联 | 每实例 5 个全局 bus 订阅从不 unsubscribe；实例缓存无空闲回收 -> 5N 闭包常驻 | 同上 |
| 存储膨胀 | SQLite 1.99GB 无清理策略 | issue #16729 |
| 修复范式 | 队列加容量 + drop-oldest + 慢客户端断开；内存 telemetry 周期采样 + SIGUSR1 触发 heap dump | 对应修复 PR 分支 |

### kxen 内存设计（E1-E8）

| # | 决策 | 依据 |
| --- | --- | --- |
| E1 | 进程级内存预算：周期采样 `memoryUsage()`，超水位依次执行「display 层驱逐 -> 提前压缩 -> 拒绝新 subagent」，watchdog 写事件流 | CC 无总预算的教训 |
| E2 | 三层分离：context（发给 API）/ display（TUI 状态，环形缓冲有界）/ storage（磁盘）；display 与 context 不共享大对象引用 | CC mutableMessages 教训 |
| E3 | 工具结果落盘阈值低（>50KB 即 content-addressed 落盘，内存只留引用 + 摘要） | CC 400KB 阈值教训；与 T3 截断落盘一致 |
| E4 | 所有流必须消费完或显式 cancel；封装 fetch 层在 release 时强制 drain | CC ArrayBuffer 泄漏教训 |
| E5 | 事件队列有界 + 慢消费者断开 + drop-oldest 策略 | OpenCode 187GB 事故 |
| E6 | 订阅生命周期绑定 owner（subagent / session / workflow run 销毁即强制 unsub），静态审计禁止游离 subscribe | OpenCode 闭包泄漏教训 |
| E7 | subagent 内存随生命周期释放；swarm 场景进程级并发上限同时受内存水位约束（不止受 provider 并发约束） | CC 并发子代理放大泄漏的教训 |
| E8 | telemetry 内置：周期采样 RSS / heap / 对象计数，SIGUSR1 触发 heap dump，事件流可回放事故现场 | OpenCode 修复范式 |

## 2. 性能：与已有决策的对齐与补充

已有（design/04）：TS + Bun 为主，热点按需 Rust N-API；工具层接口预留可替换实现。补充：

- 快照 / diff 类事件的 payload 用引用（content hash）而不是完整字符串，事件总线只传 id（OpenCode diff 事故的直接教训）
- TUI 用差分渲染（pi-tui 已有），大输出走增量 append，不重绘全屏
- 摘要 / repo map / mermaid 渲染等重活放 worker 线程或原生层，不阻塞交互线程
- 启动期懒初始化：provider / LSP / tree-sitter 按需加载，`kxen --version` 这类快路径不得初始化全量模块

## 3. mermaid 内置渲染

### 各路径对比（已核实）

| 路径 | 代表 | 评价 |
| --- | --- | --- |
| mermaid-cli (mmdc) 子进程 | mermaidcat、gemini-cli issue #20393 提案 | 依赖 puppeteer / chromium，重、慢、内存高，否决 |
| mermaid.js + jsdom 进程内 | mermkit embedded | jsdom 重，启动慢，作备选 |
| 纯 Rust 渲染 | grok-build vendored `dagre_rust` + `graphlib_rust` + `mermaid-to-svg`（third_party/）；subinium 用 `mermaid-rs-renderer` + `resvg` | 无浏览器依赖、快、可进 N-API，与 kxen 原生路线一致，选定 |

### 终端显示协议（已核实）

- kitty graphics protocol：kitty / WezTerm / Ghostty 原生；zstd 压缩 + 内容 hash 缓存（subinium 实践）
- iTerm2 OSC 1337：iTerm2 / Warp / VS Code 终端
- chafa / ASCII box-drawing 兜底：不支持图像协议的终端降级为字符画
- `AGENT_GRAPHICS` 约定（https://github.com/remorses/kitty-graphics-agent ）：agent 在 bash 环境注入 `AGENT_GRAPHICS=kitty`，子进程 CLI 据此向 stdout 发 kitty 图像序列，harness 拦截、剥离、作为附件注入模型上下文。kxen 的 bash 工具应支持该约定：我们自己的工具链（包括 kxen 产的 CLI）与子进程生态可以零额外调用把图传给模型

### kxen mermaid 决策（M1-M4）

| # | 决策 |
| --- | --- |
| M1 | 内置 `render_mermaid` 工具（setting-gated）：模型生成 mermaid -> N-API 纯 Rust 渲染 SVG -> resvg 栅格化 PNG |
| M2 | 显示分档：kitty 协议（首选）-> iTerm2 OSC1337 -> ASCII 兜底；结果按源 hash 缓存 |
| M3 | 给模型返回「已展示 + 尺寸 + 是否降级」摘要，不把 PNG 塞回上下文（除非模型主动 `read_image`） |
| M4 | bash 工具支持 `AGENT_GRAPHICS` 拦截约定，子进程图像进上下文走统一附件管道 |

## 4. 可观测性与调试体验（DX）

- 事件流是唯一真相：模型调用、工具调用、降级、纠偏、压缩、内存水位事件全部进统一事件流（OpenHands event log 思路），TUI 的任何视图都是事件流的投影
- `/context` 分类统计（Claude Code / OMP）、`/debug memory` 实时占用与 top retainers
- 请求级 dump 可选开启（Cahciua 把请求 / 响应 JSON 落 /tmp 的做法，默认关）
- 崩溃安全：步骤先持久化再检查中断（DCP 规则），任何意外退出后 `kxen resume` 能回到最近一致点

## 5. 对里程碑的修订

- M0: E2 三层分离、E4 fetch 封装、懒初始化从骨架期就位（事后补最贵）
- M1: E1 内存预算 + E5 有界队列 + E6 订阅生命周期审计
- M2: E7 subagent 内存约束、E8 telemetry
- M5: M1-M4 mermaid（随原生层一起上）
