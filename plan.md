# kxen 产品官网与文档计划

- 状态: CONFIRMED
- 分支: `docs/nimbus-site`
- 基线 commit: `1c2fd2c7f50e223508ab3aad8fe576409c01e28c`
- 隔离工作区: `file:///Users/xiaobai/Code/SelfCode/kxen-docs-nimbus`
- 目标: 使用 Cloudflare Nimbus 建设 `https://kxen.ai`，官网就是产品文档

## 1. 边界

- 不修改应用功能。
- 不修改根应用依赖和锁文件。
- 不吸收主工作区其他 Agent 的未提交修改。
- 网站全部内容位于 `file:///Users/xiaobai/Code/SelfCode/kxen/website/`。
- 根 `file:///Users/xiaobai/Code/SelfCode/kxen/docs/` 移除。
- 开发 research、analysis、plan、旧 PRD、旧设计、旧规则和内部 QA 不进入产品站。
- 已确认的产品事实沉淀到公开权威页面。
- 工程命令和 Agent 知识保留在根 `README.md`、`.agents` 和各模块源码注释中。

## 2. 产品内容

- [x] 首页和产品定位
- [x] 当前状态和可用性
- [x] 平台要求
- [x] Workspace、Session 和 Composer
- [x] Provider、账号和模型路由
- [x] Goal、Workflow、Subagent 和 Agent Teams
- [x] Worktree、Checkpoint 和 Rewind
- [x] Knowledge、Rules、Skills 和 Memory
- [x] MCP、LSP、Browser、Voice 和 Schedule
- [x] 配置、工具、Safety、存储、快捷键和排障
- [x] Runtime、Context Engineering、MRM、Orchestration 和 Security Model

## 3. 网站能力

- [x] Cloudflare Nimbus
- [x] kxen 图标和应用主题
- [x] 简体中文
- [x] 搜索和响应式导航
- [x] 404、sitemap、robots 和 Open Graph
- [x] `https://kxen.ai/llms.txt`
- [x] `https://kxen.ai/llms-full.txt`
- [x] 每页 Markdown 和 MDX alternate

## 4. 验证

- [x] Nimbus frontmatter 和内部链接
- [x] 单 H1、标题层级和代码块语言
- [x] 字符白名单
- [x] `pnpm lint:docs`
- [x] `pnpm typecheck`
- [x] `pnpm build`
- [x] Pagefind index、404 和 Agent surfaces 静态及 HTTP 检查
- [x] 根应用依赖和运行逻辑零改动
- [ ] Chrome 桌面视口和移动视口，SKIP: 当前会话未配置 `chrome-devtools` MCP
- [ ] Chrome 搜索交互，SKIP: 当前会话未配置 `chrome-devtools` MCP

## 5. 发布

- [x] 确认 `https://kxen.ai` 所在 Cloudflare account: Qingmu，Account ID `86e4d320a5d69fb54f9721fb219a4902`
- [x] 配置 Workers Static Assets、Qingmu Account ID 和 `https://kxen.ai` Custom Domain
- [x] 关联 Cloudflare Workers Builds: `main` -> `website` -> `pnpm run build` -> `npx wrangler deploy`
- [x] 通过 Cloudflare Workers Builds 部署 Workers Static Assets
- [x] 绑定 `https://kxen.ai`
- [x] 验证 HTTPS、DNS、页面、Pagefind assets 和 Agent surfaces
