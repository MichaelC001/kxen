# kxen 产品官网与文档计划

- 状态: COMPLETED
- 分支: `docs/website-capabilities`
- 基线 commit: `c5ab71bbf5659bece107f2cd1c32425617436f1e`
- 隔离工作区: `file:///Users/xiaobai/Code/SelfCode/kxen-docs-capabilities`
- 目标: 将 `https://kxen.ai` 重构为按用户旅程组织的产品官网与权威文档

## 1. 边界

- 不修改应用功能。
- 不修改根应用依赖和锁文件。
- 不吸收主工作区其他 Agent 的未提交修改。
- 网站全部内容位于 `file:///Users/xiaobai/Code/SelfCode/kxen/website/`。
- 根 `file:///Users/xiaobai/Code/SelfCode/kxen/docs/` 移除。
- 开发 research、analysis、plan、旧 PRD、旧设计、旧规则和内部 QA 不进入产品站。
- 已确认的产品事实沉淀到公开权威页面。
- 工程命令和 Agent 知识保留在根工程说明、Agent 知识目录和各模块源码注释中。
- 一个页面只介绍一项用户可感知能力。
- 一级栏目只负责分类和导航，不承担多个能力的合并说明。
- 页面统一说明能力目标、产品入口、使用方式、作用范围、状态和限制。

## 2. 产品内容

- [x] 首页和产品定位
- [x] 当前状态和可用性
- [x] 平台要求
- [x] Workspace、Workspaces 看板、Session、Composer 和上下文独立页面
- [x] Provider、账号、模型、模型路由和用量独立页面
- [x] Goal、Workflow、Subagent、Agent Teams、后台任务、工具、Safety 和 Approval 独立页面
- [x] Knowledge Library、Rules、References、Skills、Commands、Notes 和 Memory 独立页面
- [x] MCP、LSP、Browser、Voice 和 Schedule 独立页面
- [x] Checkpoint、Rewind 和 Worktree 独立页面
- [x] 配置、数据存储、快捷键、诊断和故障排查独立页面
- [x] Runtime、Context Engineering、MRM、Orchestration 和 Security Model 原理页面

## 3. 网站能力

- [x] Cloudflare Nimbus
- [x] 使用 Tauri bundle 当前图标统一 Header、首页、favicon 和 Open Graph
- [x] 简体中文
- [x] 搜索和响应式导航
- [x] 404、sitemap、robots 和 Open Graph
- [x] `https://kxen.ai/llms.txt`
- [x] `https://kxen.ai/llms-full.txt`
- [x] 每页 Markdown 和 MDX alternate
- [x] Mermaid 图表按需加载、明暗主题、错误状态和全屏查看

## 4. 验证

- [x] Nimbus frontmatter 和内部链接
- [x] 单 H1、标题层级和代码块语言
- [x] 字符白名单
- [x] `pnpm lint:docs`
- [x] `pnpm typecheck`
- [x] `pnpm build`
- [x] Pagefind index、404 和 Agent surfaces 静态及 HTTP 检查
- [x] 8 个 Mermaid 图表页面、动态资源、Markdown twins 和 BOM 检查
- [ ] Browser 桌面视口、移动视口和搜索交互，SKIP: 当前没有可用 Browser 或 Chrome 连接
- [x] 根应用依赖和运行逻辑零改动

## 5. 发布

- [x] 确认 `https://kxen.ai` 所在 Cloudflare account: Qingmu，Account ID `86e4d320a5d69fb54f9721fb219a4902`
- [x] 配置 Workers Static Assets、Qingmu Account ID 和 `https://kxen.ai` Custom Domain
- [x] 关联 Cloudflare Workers Builds: `main` -> `website` -> `pnpm run build` -> `npx wrangler deploy`
- [x] 通过 Cloudflare Workers Builds 部署 Workers Static Assets
- [x] 绑定 `https://kxen.ai`
- [x] 提交并 fast-forward 推送 `main`
- [x] 验证 Cloudflare Workers Builds 自动发布
- [x] 验证 HTTPS、页面、Pagefind assets 和 Agent surfaces
