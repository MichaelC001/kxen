# kxen 迁移规划：基于 OpenCode 源码重构

版本: 1.1
日期: 2026-07-20
状态: 待确认执行

## 1. 总体策略

- 源码迁移，不保留 opencode 的 git 历史（拷贝大于合并）。
- kxen 仓库 = 主仓库（代码 + 文档），StringKe/opencode fork 仅作上游同步桥梁。
- 遵守 bun monorepo（workspaces + catalog + turbo），沿用上游工程结构做精简和调整。
- 上游同步走 sync-point 标记 + fork 内 format-patch + protected-features 清单（OMP porting 模式）。

## 2. 仓库拓扑

```
StringKe/kxen        主仓库：packages/ + docs/ + 工具链
StringKe/opencode    fork：跟踪 anomalyco/opencode，仅用于生成同步 patch
```

本地布局：

```
/Users/xiaobai/Code/SelfCode/kxen       主仓库
/Users/xiaobai/Code/SelfCode/opencode   fork 的本地 clone（仅同步时使用，不参与开发）
```

## 3. 上游包清单与取舍

数据来源: `gh api repos/anomalyco/opencode/contents/packages`（2026-07-20，dev 分支）

### 3.1 保留（核心链路）

| 包 | 作用 | kxen 角色 |
| --- | --- | --- |
| packages/opencode | 主引擎：server/session/tool/agent/provider/mcp/lsp/config/auth/cli/worktree/snapshot/permission/skill/command/storage/background | daemon 本体 |
| packages/core | session runner、system-context、effect 层 | 依赖 |
| packages/server | HTTP API（Hono + OpenAPI） | daemon API |
| packages/llm | provider/protocol 适配（anthropic/openai/xai/bedrock 等） | MRM 挂载点 |
| packages/app | 浏览器 GUI（Solid + Vite），根 `dev:web` 指向它 | kxen 主界面 |
| packages/ui | Solid 组件库 | app 依赖 |
| packages/session-ui | 会话 UI 组件（markdown-stream 等） | app 依赖 |
| packages/schema | 共享 schema | 依赖 |
| packages/protocol | 协议定义 | 依赖 |
| packages/client | 生成客户端 | 依赖 |
| packages/plugin | 插件系统 | 保留（hooks/扩展） |
| packages/sdk + sdk/js | JS SDK | 保留 |
| packages/sdk-next | 下一代 SDK | 保留，后续评估 |
| packages/script | 构建/脚本 | 保留 |
| packages/http-recorder | HTTP 录制回放（测试） | 保留 |
| packages/httpapi-codegen | API 代码生成 | 保留 |
| packages/effect-drizzle-sqlite | 存储 | 保留 |
| packages/effect-sqlite-node | 存储 | 保留 |
| packages/codemode | Effect 沙箱代码执行 | 暂留，评估后决定去留 |

### 3.2 删除（与 kxen 形态无关）

| 包 | 删除理由 |
| --- | --- |
| packages/tui | 不要终端 UI；主包 run/attach/config/plugin 的 tui 渲染链路依赖它，M1 暂留保 install 绿，M4 随 CLI 精简一起切除 |
| packages/cli | TUI 入口（bin lildax），依赖 tui |
| packages/desktop | 不要 Electron 桌面版 |
| packages/web | Astro 官网/文档站 |
| packages/console + packages/console/* | 云端控制台（SST） |
| packages/stats + packages/stats/* | 统计服务 |
| packages/enterprise | 企业版 |
| packages/slack | Slack 集成 |
| packages/function | GitHub App 函数 |
| packages/storybook | 组件展示 |
| packages/docs | 上游文档站内容 |
| packages/containers | 容器定义 |
| packages/identity | 无独立 package.json，随关联包评估 |

### 3.3 主引擎内部子系统取舍（packages/opencode/src）

删除: `acp/`（Agent Client Protocol，含 cli/cmd/acp.ts 与 test/acp/）、`ide/`（单文件无引用）、`sync/`（单文件无引用）。

保留: `server/`、`session/`、`tool/`、`agent/`、`provider/`、`mcp/`、`lsp/`、`config/`、`auth/`、`cli/`（精简）、`worktree/`、`snapshot/`、`permission/`、`skill/`、`command/`、`storage/`、`git/`、`background/`、`question/`、`bus/`、`project/`、`plugin/`、`patch/`、`format/`、`image/`、`env/`、`id/`、`installation/`、`util/`、`account/`（本地账号层，裁剪云功能后保留凭证部分）。

保留但禁用/修正（M3 实地定性后调整）:

- `control-plane/` 保留。实地定性为 workspace 抽象层（WorkspaceAdapter / 多 workspace 路由），不是云端控制台，worktree 与 server 路由依赖它。
- `share/` 保留代码，运行时禁用。上游自带 `OPENCODE_DISABLE_SHARE` 开关（share-next.ts），M4 在启动入口默认注入；删除会动 server 路由与 SDK 契约，成本高于收益。

### 3.4 根目录取舍

删除: `README.*.md` 全部翻译版本（24 个，保留单一 README.md 改写为 kxen）、`screenshot-uk.png`、`nix/`、`flake.*`、`infra/`、`sst.config.ts`、`sst-env.d.ts`、`artifacts/`、`specs/`（评估后定）、`.vscode/`、`.zed/`、`github/`（评估后定）、`.gitleaksignore`、上游 `AGENTS.md`（改名为 `UPSTREAM-AGENTS.md` 留存参考，kxen 自己的 AGENTS.md 保持生效）、`CONTEXT.md`（保留，是上游架构术语表，改名 `docs/upstream/CONTEXT.md`）。

保留: `packages/`（精简后）、`patches/`（patchedDependencies 需要）、`script/`、`perf/`（评估）、`bunfig.toml`、`bun.lock`（重新生成）、`turbo.json`、`tsconfig.json`、`.husky/`（评估后定）、`.editorconfig`、`.gitattributes`、`.oxlintrc.json`、`.prettierignore`、`install`（评估）、`sdks/`（评估后定）、`SECURITY.md`（改写）、`CONTRIBUTING.md`（删除或改写）、`STATS.md`（删除）、`LICENSE`（MIT 保留，追加 kxen 版权行）。

### 3.5 主引擎改造项（kxen 特定需求，随 M6 或更早插入）

- exec 工具: 按 docs/analysis/09（X1-X6）改造 tool/ 层的 shell 工具为 `exec(type: zsh|bash|fish|cmd|powershell, path, bin, ...args)`，让模型显式选择 shell 方言，避免组合长命令一把梭；配套输出减载与误用纠偏（docs/analysis/02 T1-T12）。
- 系统提示词注入: system-context 层按 docs/analysis/07（P1-P11）实现分层组装，注入 kxen 自有提示词；不做内容级风控（逆向/破解/外挂类请求不拦截），仅在执行层拦灾难操作。
- context 工程: session/compaction 按 docs/analysis/01（C1-C9）调优，优先复用上游 Context Epoch / Mid-Conversation System Message 机制（见 docs/upstream/CONTEXT.md 术语表）。
- mermaid: app 的 markdown 渲染链路（marked）接入 mermaid 浏览器原生渲染，终端渲染方案作废。
- LSP/MCP auto-detect: 沿用上游 lsp/ mcp/ 的自动探测能力，按 docs/analysis/09（L1-L6）评估补强。

## 4. 重构调整项

### 4.1 品牌与命名（已定：全量改）

- 内部包名 `@opencode-ai/*` 全部改为 `@kxen/*`（用户 2026-07-20 决策，品牌彻底干净）。
- 用户面改动：bin 名 `opencode` -> `kxen`、CLI 名称、GUI 标题/logo、配置目录 `~/.config/opencode` -> `~/.config/kxen`、数据目录 `~/.local/share/opencode` -> `~/.local/share/kxen`。
- 代价与对策：此后上游 patch 无法直接 `git am`，同步流程必须带 scope 批量替换步骤（见第 5 节），替换规则集中在 `script/sync-scope.ts` 一处维护。

### 4.2 CLI 精简

目标命令集（维护命令 only）:

- `kxen`（= start）: 启动 daemon + 打开浏览器
- `kxen start` / `kxen stop`: daemon 生命周期
- `kxen version`
- `kxen doctor`: 环境自检（bun 版本、订阅凭证状态、端口占用）
- `kxen upgrade`: 自更新

移除: TUI 入口、ACP、serve/web 的公开参数面（保留内部使用）、`auth login` 的交互流改为凭证导入 + OAuth 跳转。`-p` headless 单发在 M4 定去留（倾向作为 daemon 客户端调用保留，待用户拍板）。

### 4.3 配置

- 用户配置走 TOML（`~/.config/kxen/config.toml`），opencode 原 JSON 配置读取层做适配或替换。
- 项目级配置 `.kxen/config.toml` + `.agents/` 目录（OKF 规范，见 docs/design/09）。
- hooks 与 statusline: 全面管控 + 可选部分开启（docs/analysis/08），配置项落在 config.toml，实现挂在 plugin 包的 hooks 机制上。

### 4.4 kxen 编排层落点（新包）

历史实现参考: kxen 仓库 git 历史 d6c8ddd 之前的 packages/（M0-M5 完整实现，逻辑可复用，需适配 opencode 架构）。

| 新包 | 职责 | 与 opencode 的集成点 |
| --- | --- | --- |
| packages/kxen-mrm | 全局模型资源管理（并发/速率/配额/角色路由/降级），先按单 provider 限流，变体感知随变体支持后补 | llm 包 provider 选择路径之上 |
| packages/kxen-goal | goal 生命周期（create/active/pause/resume/complete/blocked/budget） | session 层 + server API |
| packages/kxen-workflow | 动态 workflow runtime（模型自主写脚本并执行、fan-out、pipeline、恢复；关键词触发，非 slash 命令） | agent 工具层 |
| packages/kxen-subagent | 角色化 subagent 调度（thinking/planning/execution/review/research），模型交互后自主派发 | agent 层 |
| packages/kxen-safety | 灾难操作拦截（毁系统/毁用户目录/删 git 仓库等，docs/design/05 规则族） | tool/permission 层 |
| packages/kxen-agents | .agents 目录 + OKF 解析（rules/references 类型路由、多层目录） | system-context 注入 |
| packages/kxen-auth | 订阅凭证导入（Claude Keychain / codex auth.json / grok auth.json / kimi credentials），provider 不做白名单过滤（opencode 支持的全部可用），单变体先行、变体（区域/渠道/云平台）后补，导入优先于 OAuth 流 | auth 层 |

git/worktree 不单独抽包：保持上游 `src/git/`、`src/worktree/` 模块位置，kxen 各包经主包导出引用。抽成独立包会让上游同步每次全冲突，收益抵不过成本。

## 5. 上游同步策略

1. fork（StringKe/opencode）保持与 anomalyco/opencode dev 分支同步（GitHub UI 或 `gh repo sync`）。
2. kxen 仓库根维护 `SYNC` 文件：`upstream-sha = <40位commit>`，记录当前代码对应的上游提交。
3. 同步流程（包名已全量改为 @kxen/*，patch 必须经 scope 替换才能套用）:
   - fork 拉到最新，记 `new-sha`。
   - 在 fork 本地 clone 中 `git format-patch <old-sha>..<new-sha> -- <保留包路径>`。
   - 跑 `script/sync-scope.ts` 对 patch 做批量替换（`@opencode-ai/` -> `@kxen/`、`packages/opencode` 路径引用、用户面名称），输出可套用的 patch。
   - 在 kxen 中 `git am` 或手工 port；冲突时参考 protected-features 清单。
   - 更新 SYNC 文件，提交。
4. protected-features（上游 patch 不得直接覆盖）:
   - packages/kxen-*（全部）
   - bin/CLI 入口、品牌文件、配置目录常量
   - docs/、AGENTS.md、README.md、SYNC
   - packages/opencode/src/cli/（精简后的命令面）
   - packages/opencode/src/auth/（凭证导入改动）
   - packages/opencode/src/tool/（exec 改造）
   - packages/llm/（MRM 挂载改动）
5. 分叉清单（明确不跟上游的部分）: TUI/desktop/web/console 已删，上游对这些包的更新直接忽略。

## 6. 执行顺序（里程碑）

- M1 迁移: clone fork 浅拷贝 -> 拷贝保留清单进 kxen -> 删除清单执行 -> 根 package.json 收敛（workspaces/catalog/scripts 去掉已删包）-> `bun install` 绿。
- M2 跑通: `bun run dev`（daemon）+ `bun run dev:web`（GUI）启动 -> 浏览器可建会话、可选模型、完成一次调用（先用任一已有凭证）-> 记录测试基线。
- M3 精简: src 子系统删除（acp/ide/share/control-plane/sync）-> typecheck 绿 -> 测试绿（保留包的测试）。
- M4 品牌化: bin/命令/目录/GUI 标题 -> CLI 精简为 start/stop/version/doctor/upgrade -> `-p` 去留拍板。
- M5 凭证: kxen-auth 订阅导入（单变体）-> doctor 显示各家状态 -> 每家完成一次真实调用。
- M6 编排层: kxen-mrm -> kxen-subagent -> kxen-goal -> kxen-workflow -> kxen-safety -> kxen-agents，逐个接入并验证；3.5 改造项（exec/提示词/context/mermaid）随行插入。
- M7 同步机制: SYNC 文件 + docs/upstream-sync.md + 试跑一次同步流程（哪怕无新提交）。

## 7. 验证标准（Goal proof）

1. `bun install` 成功，`bun run typecheck`（turbo）全绿。
2. `bun run dev` 起 daemon，`bun run dev:web` 起 GUI，浏览器打开能创建会话并完成一次真实模型调用。
3. doctor 输出各订阅凭证状态为已导入；每家各完成一次真实 API 调用（模型名单实拉成功）。
4. 保留包测试 `bun test` 通过率不劣于迁移前基线（以 M2 跑通时记录为基线）。
5. `SYNC` 文件与 `docs/upstream-sync.md` 存在且流程可被复述执行。
6. 删除清单中的包与根文件在仓库中不存在（glob 验证）。

## 8. 风险与注意

- patchedDependencies 有 13 个补丁，拷贝时 patches/ 目录必须完整，否则 bun install 直接挂。
- postinstall 有 `fix-node-pty`（packages/core），删 core 相关东西前先确认。
- packages/app 依赖 `@solidjs/start` 的 pkg.pr.new 预览构建（catalog 里），网络环境需能拉取。
- 删除 src 子系统时先删入口引用再删代码，逐批 typecheck，别一次删穿。
- Effect.js 牵扯面广（core/server/llm 都基于它），不评估删除，原样保留。
- 上游 `AGENTS.md` 与 kxen 规则冲突时，代码风格随上游（prettier semi=false printWidth=120），助手行为随 kxen AGENTS.md。
- kimi 与 anthropic 凭证会被官方 CLI 轮换，kxen-auth 导入必须每次优先取官方 CLI 的新鲜副本，不做一次性快照。
- provider 不做白名单过滤，opencode 支持的全部可用；变体支持（区域/渠道/云平台）为后续迭代，不在本 goal 范围。
