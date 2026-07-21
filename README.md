# kxen

kxen 是一个开源 Coding Agent Harness：daemon + 浏览器 GUI 形态，混用 Claude / Codex / Grok / Kimi 订阅（非 API 按量），融合 Claude Code 的 Dynamic Workflow 与 Kimi Code 的 Goal 生命周期，自带全局模型资源调度与灾难操作硬防护。

基于 OpenCode（https://github.com/anomalyco/opencode ，MIT）源码迁移重构，上游同步流程见 `docs/upstream-sync.md`。

## 形态

- **daemon**：`kxen` 启动（= `kxen start`），后台服务 + 自动打开浏览器 GUI
- **GUI**：浏览器访问（默认 http://localhost:3000 ），Solid 前端，mermaid 原生渲染
- **CLI**（维护命令 only）：`kxen start` / `kxen stop` / `kxen doctor` / `kxen version` / `kxen upgrade`

## 能力

- **四订阅混用**：Claude Pro/Max、ChatGPT Plus/Pro（codex）、SuperGrok（grok-build）、Kimi Code。启动时自动从官方 CLI 凭证导入（新鲜副本优先，处理轮换），`kxen doctor` 查看状态。opencode 原生支持的 provider 不做白名单裁剪，全部可用。
- **角色化模型路由**（kxen-mrm + kxen-subagent）：thinking / planning / execution / review / research 各角色绑定不同 provider/model（`~/.config/kxen/config.toml`），并发限额 + 降级链，配置见 docs/plan/03。
- **Dynamic Workflow**（kxen-workflow）：模型自主写编排脚本并执行（`agent()` / `pipeline()` / `constraints()` / `phase()` 原语），中间结果不污染主上下文，可缓存恢复。
- **Goal 生命周期**（kxen-goal）：draft / active / paused / complete / blocked / budget_limited 状态机 + 预算 + 持久化，`/goal` API。
- **灾难防护**（kxen-safety）：毁系统 / 毁用户目录 / 删 git 仓库等操作在执行层硬拦截（F1-F5 规则族），不可被提示词覆盖；不做内容级风控。
- **.agents/ OKF**（kxen-agents）：项目知识目录（rules 注入型 + references 按需 + 渐进披露索引），规范见 docs/design/09。
- **exec 多 shell 工具**：`exec(type: zsh|bash|fish|cmd|powershell, path, command)`，方言显式化 + 校验。

## 安装

```bash
curl -fsSL https://raw.githubusercontent.com/StringKe/kxen/main/install | bash
```

或手动：

```bash
git clone --depth 1 https://github.com/StringKe/kxen ~/.kxen/source
cd ~/.kxen/source && bun install
```

## 开发

```bash
mise install         # 固定 bun 版本（.mise.toml）
bun install          # 依赖（版本集中在根 package.json 的 workspaces.catalog）
bun run dev          # daemon（开发模式）
bun run dev:web      # GUI vite dev（:3000）
bun run typecheck    # turbo 全量类型检查
bun turbo test       # 全量测试
bun run format       # oxfmt
bun run lint         # oxlint
```

## 仓库结构

- `packages/opencode` - 主引擎（daemon/server/session/tool/agent/provider）
- `packages/app` - 浏览器 GUI（Solid + Vite）
- `packages/kxen-*` - kxen 编排层（mrm / goal / workflow / subagent / safety / agents）
- `packages/core`、`packages/llm`、`packages/server` 等 - 共享基础包
- `docs/` - PRD、调研、分析、设计、迁移规划（索引见 `docs/README.md`）
- `SYNC` - 上游同步点；`docs/upstream-sync.md` - 同步流程

## 许可证

MIT（含上游 OpenCode 的 MIT 授权）
