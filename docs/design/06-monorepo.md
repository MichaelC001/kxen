# Monorepo 工程结构：Bun Workspaces + Catalog

- 日期: 2026-07-20
- 状态: 已定稿（M0 骨架按此落地）

## 1. 决策

- 包管理: Bun workspaces + catalog（不用 pnpm）
- layout: `packages/*` 平铺，包名 `@kxen/<模块>`，全部 `private: true`、`version: 0.0.0`、`type: module`
- 依赖版本: 集中在根 `package.json` 的 `workspaces.catalog`，子包以 `"catalog:"` 引用，包间引用用 `workspace:*`
- 工具链: mise 固定 bun / node 版本；biome 统一 lint + format；所有包继承根 `tsconfig.base.json`
- 发布策略: 首版全部 private；对外发布时再评估 catalog + changesets

## 2. 为什么是 bun workspaces + catalog 而不是 pnpm

| 维度 | bun workspaces + catalog | pnpm workspaces |
| --- | --- | --- |
| 与运行时同源 | 是（install / test / run / compile 一个工具） | 否，需同时维护 pnpm + bun 两套 |
| 集中版本管理 | 内置 `workspaces.catalog`，子包 `"catalog:"` 引用 | 需 pnpm-workspace.yaml 的 catalog 字段 |
| 安装速度 | 快（原生实现，全局缓存） | 快（硬链接 store） |
| 生态成熟度 | 新，但 workspaces 语义对齐 npm | 最成熟 |
| 先例 | oh-my-pi 即用此形态；pi-mono 用 npm workspaces | 通用大仓首选 |

选择理由：

- 跟随 oh-my-pi（OMP）：kxen 底座是 Pi SDK，OMP 的工程形态（bun workspaces + catalog）已被其生产环境验证，保持同构减少迁移与对比成本；pi-mono 用 npm workspaces 说明 workspaces 语义本身足够，catalog 是 bun 的超集能力
- 工具链收敛：mise 管版本、bun 管依赖与脚本、biome 管风格，不引入 pnpm 作为第四个工具
- catalog 单点升级：pi 四个包（pi-ai / pi-agent-core / pi-tui / pi-coding-agent）锁版本跟进，只需改根 `package.json` 一处

代价（接受）：

- bun workspaces 的 workspace 协议、过滤语法（`--filter`）与 pnpm 不完全一致，CI 脚本按 bun 写
- 极少数只认 pnpm 的社区工具不能用，出现时个案处理

## 3. Layout：包与设计模块映射

| 包 | 职责 | 设计依据 |
| --- | --- | --- |
| `@kxen/cli` | kxen 二进制入口（bin: kxen），命令解析与进程启动 | design/01 |
| `@kxen/core` | 会话封装、agent loop 胶水、plan/build 模式（deps: pi-ai、pi-agent-core） | design/01 |
| `@kxen/tools` | exec/read/edit/search 工具面（exec 多 shell 设计） | analysis/09 |
| `@kxen/providers` | 四订阅认证（Claude / Codex / Grok / Kimi） | research/03 |
| `@kxen/prompt` | prompt composer（P1-P11 注入层组装） | analysis/07 |
| `@kxen/tui` | goal / workflow / 资源视图（deps: pi-tui） | design/03 |
| `@kxen/goal` | goal 引擎：状态机 + contract + 验证循环 | design/03 |
| `@kxen/workflow` | dynamic workflow runtime：脚本执行 + 缓存恢复 | design/03 |
| `@kxen/subagent` | subagent 管理：typed 结果 + 隔离 + IR 存储 | analysis/05 |
| `@kxen/router` | 角色化模型路由 + fallback 链 | design/02 |
| `@kxen/resources` | Model Resource Manager（并发 / 速率 / 配额 / 预算） | design/02、analysis/03 |
| `@kxen/context` | context pipeline：clearing / compaction / compose | analysis/01、analysis/05 |
| `@kxen/lsp` | 原生 LSP 客户端 + auto-detect | analysis/09 |
| `@kxen/ext-api` | 扩展 API（Pi 形态 extensions / skills / packages） | research/02 |

依赖方向：cli -> core / tools / prompt / tui；core -> pi-ai / pi-agent-core；tui -> pi-tui。编排层包之间暂不互依，后续按模块边界（design/01）补。

## 4. 版本与发布策略

- 骨架期与 M 系列里程碑期间：所有包 `0.0.0` + `private: true`，不发 npm；版本语义由 git tag 表达
- pi SDK 升级只在 catalog 改一处，配合 `bun update` 跟进
- 对外发布时（首版公开）再评估：catalog 与 changesets 的兼容方案、`bun publish` 逐个发 vs 单文件可执行分发（design/04 已定主分发形态为 `bun build --compile`）

## 5. 工具链

- 版本固定: `mise.toml`（bun 1.3.14、node 22.23.1，node 仅作兼容兜底，运行与构建以 bun 为准）
- 依赖: `bun install`；版本全部在根 catalog（typescript 7.0.2、@biomejs/biome 2.5.4、@types/bun 1.3.14、@types/node 26.1.1、pi 四包 0.80.10，均为 2026-07-20 npm registry latest）
- lint / format: biome（tabs 缩进、单引号、semicolons always、recommended 规则，见根 `biome.json`）
- 类型: strict TypeScript，全仓继承 `tsconfig.base.json`（module ESNext、moduleResolution bundler、verbatimModuleSyntax、noUncheckedIndexedAccess、noEmit、skipLibCheck）
- 测试: `bun test`

## 6. 预留：crates/

性能热点按需引入 Rust N-API（分层策略见 design/04 第 4 节）。届时在仓库根新增 `crates/` 目录，与 `packages/` 平级；TS 侧以 `@kxen/natives-<platform>` 可选依赖形态引用预编译二进制。引入前所有包保持纯 TS，`packages/` 结构不需要为此调整。
