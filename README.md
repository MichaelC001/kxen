# kxen

kxen 是一个开源的终端原生 Coding Agent Harness：混用 Claude / Codex / Grok / Kimi 四个订阅，融合 Claude Code 的 Dynamic Workflow 与 Kimi Code 的 Goal 生命周期，自带全局模型资源调度。

## 仓库结构

- `docs/` - PRD、外部调研、维度分析、设计决策（索引见 `docs/README.md`）
- `packages/` - monorepo 包（bun workspaces + catalog，包与设计模块的映射见 `docs/design/06-monorepo.md`）

## 开发

```bash
mise install     # 固定 bun / node 版本
bun install      # 安装依赖（版本集中在根 package.json 的 workspaces.catalog）
bun run check    # biome lint + format 检查
bun run typecheck
bun test
```

设计与决策文档入口: `docs/README.md`
