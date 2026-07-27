# kxen 产品官网

## 边界

- 官网就是产品文档。
- 只写产品定位、可用性、上手、使用指南、Reference 和核心概念。
- 不保存开发 research、analysis、plan、旧 PRD、旧设计、内部 QA 和实现过程。
- 网站依赖、配置、源码和构建产物全部保存在 `website` package。
- 不修改根应用依赖。

## 内容

- `overview`: 产品定位和状态。
- `getting-started`: 可用性、系统要求和首次使用。
- `guides`: 面向任务的产品指南。
- `reference`: 当前产品行为和配置。
- `concepts`: 用户需要理解的 Runtime 概念。

页面 H1 由 frontmatter `title` 生成，正文不重复 H1。每页必须有 `description` 和 `status`。

## 修改流程

1. 读取当前源码和现有产品文档。
2. 修改对应产品页面。
3. 搜索全站相同模式。
4. 运行 `pnpm check`。
5. 检查生产构建产物和页面。

## 命令

```bash
pnpm dev
pnpm lint:docs
pnpm typecheck
pnpm build
pnpm check
```

## Nimbus

- 组件必须注册到 `src/components.ts`。
- 保留 `AgentDirective`。
- 保留每页 Markdown alternate、`llms.txt`、`llms-full.txt`、Pagefind、sitemap、robots、404 和 Open Graph。
- Nimbus 项目: [https://github.com/cloudflare/nimbus](https://github.com/cloudflare/nimbus)
- Nimbus 文档: [https://nimbus-docs.com](https://nimbus-docs.com)
