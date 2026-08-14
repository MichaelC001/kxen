# Changelog

此文件由 [git-cliff](https://github.com/orhun/git-cliff) 根据 Git tag 和 Conventional Commits 自动生成，请勿手动编辑。

## [0.1.11] - 2026-08-14

### 文档

- Document isolated agent credentials ([5b04242](https://github.com/StringKe/kxen/commit/5b04242f94593d0651a91adbff52b7c0d7208a45))

### 问题修复

- **agent:** Isolate provider credentials from tools ([4750da8](https://github.com/StringKe/kxen/commit/4750da8c336428c9e347061d1f0c1cbb5867f5d2))
- **actions:** Isolate autonomous model jobs ([6df9527](https://github.com/StringKe/kxen/commit/6df9527f9ab4534056a5ebc120f535c9bca2f3b8))
- **actions:** Prevent workflow command injection ([6135b25](https://github.com/StringKe/kxen/commit/6135b25fd9ad0a887b095aedb5b860b4773df098))

## [0.1.10] - 2026-08-14

> **版本主题:** Autonomous Issue Remediation

### 本次更新

- kxen-agent can verify, implement, independently review, and publish bounded GitHub Issue fixes through isolated runners. ([3b87934](https://github.com/StringKe/kxen/commit/3b879348d8219ad32a4f60018bf350c61059a392))
- GitHub repository automation now keeps platform operations outside DCPAgent and separates model credentials from write credentials. ([3b87934](https://github.com/StringKe/kxen/commit/3b879348d8219ad32a4f60018bf350c61059a392))
- Desktop and headless assets build in parallel while preserving full signature, checksum, and updater verification. ([3b87934](https://github.com/StringKe/kxen/commit/3b879348d8219ad32a4f60018bf350c61059a392))
- xAI roles default to the verified Grok 4.6 model. ([3b87934](https://github.com/StringKe/kxen/commit/3b879348d8219ad32a4f60018bf350c61059a392))

### 性能优化

- **release:** Parallelize desktop and headless assets ([f330388](https://github.com/StringKe/kxen/commit/f330388b96439afe795892d567d3f37bdb977efd))

### 文档

- Explain DCP and autonomous issue repair ([85ae29e](https://github.com/StringKe/kxen/commit/85ae29ea5140ea24654966d1bf78f8af24b03cd4))

### 新增功能

- **models:** Default xAI to Grok 4.6 ([6a69bd9](https://github.com/StringKe/kxen/commit/6a69bd9edae1969948e5a60d4499970bcfa87de8))
- **agent:** Add reviewed GitHub issue automation ([59c4240](https://github.com/StringKe/kxen/commit/59c424020fdb61b4f7c849c8b83cd1bd09acb020))

### 问题修复

- **agent:** Harden headless shell execution ([c501a69](https://github.com/StringKe/kxen/commit/c501a695b0bd80aba4d93de5bf5e25efe8ddecdb))

## [0.1.9] - 2026-08-14

> **版本主题:** DCPAgent 安全恢复与可信发布链路

### 本次更新

- kxen-agent 在恢复时保留 UNKNOWN 工具操作并进入 input_required，避免误判成功或重复执行副作用。 ([8c7393a](https://github.com/StringKe/kxen/commit/8c7393a1f12f5815dc8fcbd8ce6ccb6f83fe7856))
- DCPAgent Session 和 bundle 会校验 definition hash、capability lock、run 与 tool journal 完整性。 ([8c7393a](https://github.com/StringKe/kxen/commit/8c7393a1f12f5815dc8fcbd8ce6ccb6f83fe7856))
- 工具子进程默认不再继承 SSH、GPG 等 credential socket，只有显式 pass-env 才会透传非 Provider 凭证。 ([8c7393a](https://github.com/StringKe/kxen/commit/8c7393a1f12f5815dc8fcbd8ce6ccb6f83fe7856))
- Web 模式的自定义 token 支持空格、符号和 Unicode 等完整 URL 编码字符。 ([8c7393a](https://github.com/StringKe/kxen/commit/8c7393a1f12f5815dc8fcbd8ce6ccb6f83fe7856))
- Website 新增 DCP 权威概念页，并同步 kxen-agent 的多平台下载、运行边界与恢复模型。 ([8c7393a](https://github.com/StringKe/kxen/commit/8c7393a1f12f5815dc8fcbd8ce6ccb6f83fe7856))
- Release 现在自动发布与 immutable tag 同源的 linux/amd64 和 linux/arm64 GHCR 镜像。 ([8c7393a](https://github.com/StringKe/kxen/commit/8c7393a1f12f5815dc8fcbd8ce6ccb6f83fe7856))

### 工程

- **release:** Bind Docker image to immutable release ([6b8388f](https://github.com/StringKe/kxen/commit/6b8388f04e17c7285faf17d591aa580eb8801c3f))

### 文档

- **changelog:** Complete v0.1.8 notes ([4f4d294](https://github.com/StringKe/kxen/commit/4f4d29424401929f4b32a9c80e62969ffafd0371))
- Establish DCP as a core concept ([1e1aaa8](https://github.com/StringKe/kxen/commit/1e1aaa852b27cfc669f3f313ab47068c856ea979))

### 架构与重构

- **core:** Distinguish DCPAgent from Kanban agents ([135bd14](https://github.com/StringKe/kxen/commit/135bd14f6ab1f74754d4d306e164e57754c4d5a6))
- **agent:** Split DCP store tests ([b2e66a9](https://github.com/StringKe/kxen/commit/b2e66a935177ef88004775e0d8efeaadd1d7d111))
- **agent:** Keep DCP runtime within size gate ([5818a71](https://github.com/StringKe/kxen/commit/5818a71b38dd1ad1958a3de1f2c922df04ba32af))

### 维护

- **changelog:** Sync generated history ([536bc58](https://github.com/StringKe/kxen/commit/536bc588af7c6e224b1c0a5d68f4e42bab021d86))

### 问题修复

- **agent:** Require explicit credential forwarding ([ea2d414](https://github.com/StringKe/kxen/commit/ea2d41425f012f0632c6d84b6f458d8689066f36))
- **agent:** Preserve DCP input-required events ([75a02ca](https://github.com/StringKe/kxen/commit/75a02cabae2c63fb423e9e902f97a046d07d1959))
- **agent:** Validate DCP session integrity ([a9793db](https://github.com/StringKe/kxen/commit/a9793db842914665d670ae762511b7b96a87f9db))
- **web:** Encode custom access tokens ([1ac7ae4](https://github.com/StringKe/kxen/commit/1ac7ae4168386938ae5395a42009e7c37b55e0d4))
- **release:** Stream updater archives into verifier ([3620298](https://github.com/StringKe/kxen/commit/3620298105080338be4cba4ce4096e9be1fbd1f3))
- **website:** Sanitize rendered Mermaid SVG ([ecbfcf4](https://github.com/StringKe/kxen/commit/ecbfcf4d79144cf2db6ab4cc1ef5f7a45a4c0825))

## [0.1.8] - 2026-08-14

> **版本主题:** 可扩展 OKF 知识与混合检索

### 本次更新

- 长期知识升级为 OKF v0.2 concept 模型，目录只负责组织，文件内 type 决定语义，并支持 code、refactor、test 等自定义类型。 ([6d2b2bf](https://github.com/StringKe/kxen/commit/6d2b2bf03c6da334264a836619af7ae3db0831d3))
- Rule、Skill、Command、Note、Memory、Reference 和 History 保持独立 handler，未知 type 可被发现但不会获得执行权限。 ([6d2b2bf](https://github.com/StringKe/kxen/commit/6d2b2bf03c6da334264a836619af7ae3db0831d3))
- 当前任务、涉及文件、metadata、BM25、可选 embedding 和 Markdown links 组成增量分层检索，embedding 缺失或失败时自动回退本地 BM25。 ([6d2b2bf](https://github.com/StringKe/kxen/commit/6d2b2bf03c6da334264a836619af7ae3db0831d3))
- Knowledge Library、README、Website 和项目知识已统一到 type 与 concept_id 契约，并完整覆盖管理、迁移和信任边界。 ([6d2b2bf](https://github.com/StringKe/kxen/commit/6d2b2bf03c6da334264a836619af7ae3db0831d3))

### 性能优化

- **release:** Reuse exact-commit CI and cache Rust deps ([c0ff91d](https://github.com/StringKe/kxen/commit/c0ff91db75f0bbef68296fbf44b63346bb4defdc))

### 文档

- **release:** Document accelerated verification ([16e08d9](https://github.com/StringKe/kxen/commit/16e08d975a338f195d33bb1e450a04b47d8492c9))

### 新增功能

- **knowledge:** Implement OKF concept retrieval ([d89bac0](https://github.com/StringKe/kxen/commit/d89bac00089c746db97396e6c5c70138b88d7bcd))

### 问题修复

- **knowledge:** Satisfy Rust lint gate ([faf0198](https://github.com/StringKe/kxen/commit/faf019871e3dd607c248a98c71dab6523d52d54c))

## [0.1.7] - 2026-08-13

> **版本主题:** 独立 Agent CLI 与可恢复自动化

### 本次更新

- 新增独立的 kxen-agent CLI，可从 task 动态构建或加载 DCPAgent，在本地、CI、queue worker 和轻量运行环境中执行非交互任务，无需启动 kxen server。 ([1122632](https://github.com/StringKe/kxen/commit/1122632ee1e7c42b428cdaae0173ce999eab0a65))
- DCPRun、Session、--resume、Conversation branch、Git worktree、跨 runner bundle 和 UNKNOWN tool recovery 形成可恢复的完整执行链路。 ([1122632](https://github.com/StringKe/kxen/commit/1122632ee1e7c42b428cdaae0173ce999eab0a65))
- GitHub、GitLab、Webhook 和 queue 等场景通过 MCP 或普通 CLI capability 接入，DCPAgent 核心协议保持平台无关。 ([1122632](https://github.com/StringKe/kxen/commit/1122632ee1e7c42b428cdaae0173ce999eab0a65))
- macOS、Windows 和 Linux 六个平台在同一个版本中分别提供 kxen server 与 kxen-agent 独立下载资源，并统一进入 SHA256SUMS 与签名验证链路。 ([1122632](https://github.com/StringKe/kxen/commit/1122632ee1e7c42b428cdaae0173ce999eab0a65))

### 工程

- **release:** Publish standalone agent assets ([b27623b](https://github.com/StringKe/kxen/commit/b27623beed8b216373dfcc519128d921fe0942e8))

### 文档

- **agent:** Document CLI automation and recovery ([5862a54](https://github.com/StringKe/kxen/commit/5862a5444215bb236ef0b16f58ca9878f6e6ee74))

### 新增功能

- **agent:** Add standalone DCP runtime ([2422428](https://github.com/StringKe/kxen/commit/2422428615a969d7e1eb332fc45eb597d919b544))

### 测试

- **agent:** Cover DCP recovery boundaries ([f612598](https://github.com/StringKe/kxen/commit/f61259821eec5e3c36e8805108caf524fc903c81))

### 维护

- **docs:** Format agent release documentation ([e674a38](https://github.com/StringKe/kxen/commit/e674a387f44c440c79f763fbb0e47b5ca68ff4f7))

## [0.1.6] - 2026-08-13

> **版本主题:** 持久对话分支与安全重试

### 本次更新

- 从任意消息创建持久对话分支，Sidebar 和 Session Header 会展示完整父子谱系，并支持同族切换与返回父分支。 ([e078eac](https://github.com/StringKe/kxen/commit/e078eacfa9d87299afbca90e1e8e627fdee77a3f))
- 编辑重发和重新生成现在自动进入独立分支，原始输入、图片、Context 和既有回复都会保留。 ([e078eac](https://github.com/StringKe/kxen/commit/e078eacfa9d87299afbca90e1e8e627fdee77a3f))
- 分支导出和界面明确标记对话历史独立但 Workspace 文件共享；需要文件隔离时可配合 Worktree，删除父分支也不会级联删除后代。 ([e078eac](https://github.com/StringKe/kxen/commit/e078eacfa9d87299afbca90e1e8e627fdee77a3f))

### 文档

- **session:** Document conversation branches ([a66746d](https://github.com/StringKe/kxen/commit/a66746d30c68524e8382e3bcd44ee9cc865aa437))

### 新增功能

- **session:** Persist conversation branch lineage ([a4f0776](https://github.com/StringKe/kxen/commit/a4f077660476fe9553b1c8bde113017bdb78be71))
- **ui:** Add conversation branch navigation ([0a2e3ba](https://github.com/StringKe/kxen/commit/0a2e3bab520cca89555dba2829d49c8b3428ccc6))

## [0.1.5] - 2026-08-13

> **版本主题:** Bot 自构建与工作区导航

### 本次更新

- 每个 Bot 都拥有与自身身份绑定的交互式 Builder，可通过持续对话创建或完善定义，并由 Owner 审阅授权、测试和发布。 ([acdcb38](https://github.com/StringKe/kxen/commit/acdcb385b0ddc7f0c4fbd8b0a1b5ce46909c0494))
- Bot 管理、Bot-to-Bot Direct 与 Group、持久化恢复和 Workspace 模型路由现在具有更一致的状态、权限与失败语义。 ([acdcb38](https://github.com/StringKe/kxen/commit/acdcb385b0ddc7f0c4fbd8b0a1b5ce46909c0494))
- 应用采用稳定的 Logo、Search、工作区、Bots、项目区和底部 Settings 侧边栏，主内容与右侧上下文详情不再挤入 footer。 ([acdcb38](https://github.com/StringKe/kxen/commit/acdcb385b0ddc7f0c4fbd8b0a1b5ce46909c0494))

### 工程

- **release:** Generate version-specific notes with git-cliff ([15195b2](https://github.com/StringKe/kxen/commit/15195b264733666d851794915d573a911d09aa55))
- **release:** Require product notes and exact source parity ([1fbc939](https://github.com/StringKe/kxen/commit/1fbc939a487fb6bd960850dd1748dfa0e80bc28d))

### 性能优化

- **web:** Defer Mermaid from startup bundles ([7c38193](https://github.com/StringKe/kxen/commit/7c3819387eb295822692d6d713eae9e37126fb40))

### 文档

- **release:** Document generated changelog workflow ([38233ca](https://github.com/StringKe/kxen/commit/38233ca5e0a97c8b2c94c2f68e5ee1e95618589d))
- **bot:** Document per-bot self-building ([ca13da5](https://github.com/StringKe/kxen/commit/ca13da5c65dd49995f7f6c66b4c4ea0f173a3853))
- **release:** Document stable changelog commits ([f66446e](https://github.com/StringKe/kxen/commit/f66446e3e04b195f6aa9c457d08015b4926a88be))

### 新增功能

- **bot:** Make Builder sessions conversational and bot-bound ([822b4fc](https://github.com/StringKe/kxen/commit/822b4fcf5222b5a24a1165aae18e69eda90cd356))
- **ui:** Establish persistent workspace navigation ([fd0bc25](https://github.com/StringKe/kxen/commit/fd0bc25303c9784728062eb246de7a759c956064))

### 架构与重构

- **codebase:** Enforce bounded module responsibilities ([210347a](https://github.com/StringKe/kxen/commit/210347a71fcf10a4ff40230198dacada714042c8))

### 测试

- **ui:** Align sidebar interaction contract ([eace4b4](https://github.com/StringKe/kxen/commit/eace4b489c4fa71212e6d8d9c9cc5c41ba61cf44))

### 问题修复

- **release:** Normalize generated changelog output ([9f5ac0b](https://github.com/StringKe/kxen/commit/9f5ac0b7394902fbf3b58f6c56fe26555a71c4b7))
- **bot:** Reconcile mutations against durable state ([39280e2](https://github.com/StringKe/kxen/commit/39280e29b0911ae2a66cb8a4aa7f10dcbb7ea797))
- **settings:** Report workspace routing readiness ([4a4d225](https://github.com/StringKe/kxen/commit/4a4d2251066c326a063ef5cdb52c8c57a742a1c7))
- **bot:** Reopen archived direct conversations safely ([460d7e3](https://github.com/StringKe/kxen/commit/460d7e34fd5a3e985fd081615fe8549a629867b5))
- **bot:** Scope self-builder identity to each Bot ([7006491](https://github.com/StringKe/kxen/commit/700649100a4dfc69152d8ca97b7e796fe2c436c3))
- **ui:** Unify accessible selection navigation ([e070d69](https://github.com/StringKe/kxen/commit/e070d699595395de18c1aaffbd635b71a499499a))
- **bot:** Reserve controlled tests for owners ([003b2fa](https://github.com/StringKe/kxen/commit/003b2fa20349691b9ebabdb6e69e49741b531268))
- **bot:** Make management and grants reviewable ([0b609f4](https://github.com/StringKe/kxen/commit/0b609f434c13da32b0856625f599463b1caa20e6))
- **ui:** Normalize user-visible status language ([518d0a2](https://github.com/StringKe/kxen/commit/518d0a2d1bd510c4a08204746f5d9e06b803ac0a))
- **runtime:** Treat first-start recovery as empty ([db4a720](https://github.com/StringKe/kxen/commit/db4a7205d19535b3f613820df8ac07775b2d8cd2))
- **release:** Preserve product metadata for git-cliff ([594f9e3](https://github.com/StringKe/kxen/commit/594f9e363f113ef0ca047a254a9724219eac2253))
- **release:** Harden shell validation paths ([09b2f15](https://github.com/StringKe/kxen/commit/09b2f15377ce3c8b950e9beed7d24d8ce822425a))
- **website:** Remove vulnerable archive extractor ([0050ab1](https://github.com/StringKe/kxen/commit/0050ab1831824e055dd7baacfbde41468ce0c829))

## [0.1.4] - 2026-08-12

### 文档

- **bot:** Document durable bot workflows ([af43444](https://github.com/StringKe/kxen/commit/af4344414464c13b4a7659638c156e370399137f))

### 新增功能

- **bot:** Implement durable bot collaboration ([475cec4](https://github.com/StringKe/kxen/commit/475cec49caace04e7bb1c4366c538952be2f3a20))
- **bot:** Expose management RPC and UI ([4c0657e](https://github.com/StringKe/kxen/commit/4c0657eac398b6851f737d2b2a2958b419569176))

### 架构与重构

- **core:** Add durable execution primitives ([30dc2dd](https://github.com/StringKe/kxen/commit/30dc2dd7199ba637f89a3aefd90104f4014c8ef9))
- **agent:** Add reusable DCP runtime ports ([bf29005](https://github.com/StringKe/kxen/commit/bf29005413cdb3b247943f62aa0d0b7f4e89421a))

### 测试

- **bot:** Cover management and runtime workflows ([afce0f3](https://github.com/StringKe/kxen/commit/afce0f34f37a8ab9dc41f6c7403b9a9b5b4a2d49))

### 问题修复

- **core:** Harden durable execution semantics ([7d6a8d1](https://github.com/StringKe/kxen/commit/7d6a8d1866f9f652eed3f44c6d4b61195cbc32a7))
- **bot:** Enforce runtime contracts and isolation ([b2a648b](https://github.com/StringKe/kxen/commit/b2a648bd677b620e4814bcdbad793fae83267c85))
- **bot:** Align management UI with published contracts ([9b63b1e](https://github.com/StringKe/kxen/commit/9b63b1ebb8bc69b83443cb958cca5cffe1aa1bcb))

## [0.1.3] - 2026-08-12

### 新增功能

- **composer:** Add context-aware auto suggestions ([27256e4](https://github.com/StringKe/kxen/commit/27256e4e33b670fbdb3e2bbb9430196a98f1e42c))

### 维护

- Clean residual and verbose source comments ([f836f1b](https://github.com/StringKe/kxen/commit/f836f1b88e2cc4f9a68eb76896ac13c195e8c110))
- Ignore local agent tooling and tidy gitignore ([c31fe5f](https://github.com/StringKe/kxen/commit/c31fe5fac973047de03e4406fd6229a7e398273f))
- Stop tracking gitignored generated and OS junk ([54b750f](https://github.com/StringKe/kxen/commit/54b750f5f037c3f0e0aa81c2498c6664748c71c7))

## [0.1.2] - 2026-08-11

### 依赖更新

- **deps:** Refresh compatible Cargo and npm lockfile updates ([4cf5a64](https://github.com/StringKe/kxen/commit/4cf5a6492df626d7af0222f3f9955019c012d968))
- **deps:** Bump remaining direct deps to current latest ([01e2686](https://github.com/StringKe/kxen/commit/01e26862d965a9505e987c4571335bf1de619b4f))

## [0.1.1] - 2026-08-09

### 依赖更新

- **deps:** Bump actions/upload-artifact from 6.0.0 to 7.0.1 (#16) ([5361159](https://github.com/StringKe/kxen/commit/53611597b6feda32b8149dc604058305a338f1a7))
- **deps:** Bump actions/download-artifact from 7.0.0 to 8.0.1 (#17) ([139f2e9](https://github.com/StringKe/kxen/commit/139f2e9508177071151c6a14bdd5a2d76385d415))
- **deps:** Bump pnpm/action-setup from 6.0.9 to 6.0.10 (#18) ([62d6cb2](https://github.com/StringKe/kxen/commit/62d6cb2f3d725f68813838034d23071218ea151b))
- **deps:** Bump @cloudflare/nimbus-docs in /website (#21) ([bee5314](https://github.com/StringKe/kxen/commit/bee5314f62d0db9a7833b05ddd6cded987e37fb6))
- **deps:** Bump tokio-tungstenite from 0.29.0 to 0.30.0 (#19) ([e0f9179](https://github.com/StringKe/kxen/commit/e0f9179ba5a78295520ab17fc4919df5b9210316))
- **deps:** Bump similar from 2.7.0 to 3.1.2 (#24) ([cb3c551](https://github.com/StringKe/kxen/commit/cb3c55150f3e2937c25af3e21b00b70c4563af78))
- **deps:** Bump base64 from 0.22.1 to 0.23.1 (#23) ([2f6266e](https://github.com/StringKe/kxen/commit/2f6266eab631534ab915365bdd222295998f3bbe))
- **deps:** Bump cron from 0.12.1 to 0.17.0 (#20) ([a51f8b1](https://github.com/StringKe/kxen/commit/a51f8b1d8410e056598223247e05b5c4b5cfbcee))
- **deps:** Bump sha2 from 0.10.9 to 0.11.0 (#22) ([c6548e1](https://github.com/StringKe/kxen/commit/c6548e16b1922864ada5fa6948485eb9b69ed20a))

### 性能优化

- Reduce redundant Rust allocations ([0b9fab3](https://github.com/StringKe/kxen/commit/0b9fab3b0bb01933f7bd88586f08ed9d3b9cf727))

### 文档

- **kanban:** Website 开放文档 kanban 章节与入口索引(P6) ([c0346cf](https://github.com/StringKe/kxen/commit/c0346cf7f589e573af9334a17cdf412db9f379d7))

### 新增功能

- **core:** Turn 内迭代级持久化与 tool 交互完全重建(DCP P0) ([aacf3b7](https://github.com/StringKe/kxen/commit/aacf3b7d9cecb125b8523fc9addeacfb11b56458))
- **frontend:** 时间线适配迭代级消息,同一 run 归并为一个视觉回合 ([1594713](https://github.com/StringKe/kxen/commit/15947136f81bae96b27d5f72e38c79c68503b8b5))
- **core:** Teammate 与 subagent/background 的 turn 级持久化(DCP 缺口 4/5) ([818b1db](https://github.com/StringKe/kxen/commit/818b1db9eefd70cc14e45942ab50a17f563f1a2a))
- **core:** DCP 部分符合项处置 ([5fbee8e](https://github.com/StringKe/kxen/commit/5fbee8e1e4f4d8b5b1db5877b6004019ff39dfcf))
- **kanban:** Event log 核心(P1) ([9cfd9b2](https://github.com/StringKe/kxen/commit/9cfd9b2a7efecf3061cf2a37354905b69edf1902))
- **kanban:** 列执行器与 DCP Agent 定义(P2a) ([315bb83](https://github.com/StringKe/kxen/commit/315bb83bae40da6b9a0868c4f0e32105f756f217))
- **kanban:** 主线程 Agent 的 kanban.* 工具面(P2b) ([2b9f467](https://github.com/StringKe/kxen/commit/2b9f467ff4a0341da748382de328919f7eab945a))
- **kanban:** 看板级自主授权(P3) ([ca745eb](https://github.com/StringKe/kxen/commit/ca745ebe476181c1b6455d7697a20eab0bf41ae7))
- **kanban:** 每卡 worktree 隔离(P4) ([7c72968](https://github.com/StringKe/kxen/commit/7c72968fdebf6e18eb58bfd35385ad1d24c1ccf3))
- **kanban:** RPC + topic + 前端 UI 入口(P5) ([75ce5ee](https://github.com/StringKe/kxen/commit/75ce5ee9d70dda536fdeb332447b264af9a49ac1))
- **kanban:** 落地/认领/自动放行补发 KanbanUpdate,补齐守卫不变量 ([ea829b1](https://github.com/StringKe/kxen/commit/ea829b1759c20e8973560ff1adc0799882c583b8))
- **kanban:** Custom permission_profile 支持 AI 自编写带显式工具集的 DCP agent ([7402550](https://github.com/StringKe/kxen/commit/74025500234435e97ca6bc5737f0a03d64166487))
- **agent:** Exec/task 后台进程 DCP 与 workflow 自动 run_id ([b63b76b](https://github.com/StringKe/kxen/commit/b63b76b44c2fb5fef44714101f39dbf4bd36bb9a))

### 架构与重构

- Hex-encode sha2 digests without LowerHex ([350abb9](https://github.com/StringKe/kxen/commit/350abb9559806ae9bb5d8a081c653b0e833d2f00))
- **core:** 通知落盘逻辑上移共享,CLI 与 GUI 复用 ([02dc751](https://github.com/StringKe/kxen/commit/02dc7518a693cfdfb9d24d86ca1aed1e73f3feb9))
- **kanban:** 工具名点号改下划线前缀 ([b6f0a4b](https://github.com/StringKe/kxen/commit/b6f0a4b3feb9a5deab8f45e06435e86b4a3df125))

### 测试

- **kanban:** 授权过期断言改轮询消除并行负载竞态 ([0fc46d2](https://github.com/StringKe/kxen/commit/0fc46d212b57b97a16d1a3c16bf0bd1539d2a34f))
- **kanban:** 全链路端到端与两卡 worktree 并发(P6) ([0322827](https://github.com/StringKe/kxen/commit/03228278558be500f758d21677179f90ee59d0ea))
- **llm:** Catalog panic 复位断言的轮询窗口覆盖单飞竞争最坏路径 ([c65ff87](https://github.com/StringKe/kxen/commit/c65ff87a306156e9e4b0abf69712a56f72c0e352))
- **llm:** Catalog panic 复位断言改用私有单飞 flag ([225f547](https://github.com/StringKe/kxen/commit/225f547a1a5933fdc2475224d3a9bc4e72061958))
- **kanban:** Exec 端到端按宿主可用 shell 钉方言 ([4fab0ad](https://github.com/StringKe/kxen/commit/4fab0ad943decd165bad5cf84242a33f31e38b9b))

### 维护

- **ci:** 修复发布链脚本与 workflow 审查遗留 ([57690f8](https://github.com/StringKe/kxen/commit/57690f8804f37c77774984c3dc8863dd137fa8f9))
- **kanban:** 前端与文档格式归一(vp check --fix) ([a5d5546](https://github.com/StringKe/kxen/commit/a5d55461011205fffe030819cd81efa5587b75a5))
- **core:** Keep workspace runtime within file gate ([6eec571](https://github.com/StringKe/kxen/commit/6eec571166562e4473591af1075477958db5a1a2))

### 问题修复

- **ci:** Pin valid docker action SHAs in the image workflow ([f5e3227](https://github.com/StringKe/kxen/commit/f5e3227b2fb5386619006d2a6980ae431f1577d9))
- **ci:** Lowercase the GHCR image path ([d023f7e](https://github.com/StringKe/kxen/commit/d023f7e3210b5a1ac33a11fb18ed87c9568471fc))
- **core:** 审查修复各模块正确性与安全问题 ([8d1f0cb](https://github.com/StringKe/kxen/commit/8d1f0cb6d99ca546eb69b7509dc95711842c06ff))
- **tools:** 审查修复工具层问题并拆分超门禁文件 ([b6ac0c8](https://github.com/StringKe/kxen/commit/b6ac0c838a78a407d9d8ab80cf64fe1416320100))
- **frontend:** 审查修复组件竞态与 markdown 注入面 ([074d601](https://github.com/StringKe/kxen/commit/074d60140f1c490ba90878782216ccd66b3a3dff))
- **llm:** Catalog 磁盘缓存测试改用 KXEN_CATALOG_FILE 隔离 ([fdc88c3](https://github.com/StringKe/kxen/commit/fdc88c30976749b271ff0957122193e72cb1a984))
- **kanban:** 列执行 exec 作用域、收养去重、apply 锁内补折与前缀硬化 ([0db15fe](https://github.com/StringKe/kxen/commit/0db15fe34b5e5bd5bf42fa792b58c0bd29f40d8b))
- **kanban:** 前端授权输入校验与看板订阅修正 ([105e444](https://github.com/StringKe/kxen/commit/105e444491447af2f315779b323c5070a7300c31))
- **kanban:** 事件流跨进程 flock、快照内容锚与产物快照加固 ([50260d4](https://github.com/StringKe/kxen/commit/50260d4f2e92872ca85c4a9f3441a6d67178a83f))
- **agent:** 后台子代理中断事实重启后 durable 回投父 session ([367a8cf](https://github.com/StringKe/kxen/commit/367a8cf59df1bf1c5aab0b49cf72e24542133d1f))
- **agent,kanban:** 中断补投终态识别与复审小修 ([7693b54](https://github.com/StringKe/kxen/commit/7693b542907de2c968111e323a6a197b282b62bd))
- **core:** Keep workspace validation warning-free on Windows ([02afbac](https://github.com/StringKe/kxen/commit/02afbac03860a6ba52e4f7e4e18d5e5ddfbc447e))

## [0.1.0] - 2026-08-08

### 依赖更新

- **deps:** Refresh npm and cargo dependencies within semver ranges ([5992c94](https://github.com/StringKe/kxen/commit/5992c94cf4046c09f0b17987c50b71503bf556b0))

### 工程

- **release:** Three-platform CI matrix and six-platform release pipeline ([2c5ef5d](https://github.com/StringKe/kxen/commit/2c5ef5d7416d1087a98433f47aab0e6c90082f82))
- **release:** Sign and notarize the kxen CLI for macOS ([2e45bea](https://github.com/StringKe/kxen/commit/2e45bead880ce67f5945d731e76dbb515a3ab9d9))

### 文档

- **website:** Add download flow and align docs with v0.0.1 release ([f96495e](https://github.com/StringKe/kxen/commit/f96495e22c0d6e52a63a0edf006e6ad7f2a62faa))
- Fix provider naming and supported version in changelog and security policy ([a49999e](https://github.com/StringKe/kxen/commit/a49999e6028892650261b310304eed2b63fddb2d))
- Rewrite readme as product-facing entry with download path ([bf1ca8d](https://github.com/StringKe/kxen/commit/bf1ca8db1b2d47c5818f640e000e18c42c776345))
- **website:** All-platform downloads and web mode documentation ([5db17ad](https://github.com/StringKe/kxen/commit/5db17add64b1b685f2e1f9fb52fcc6eb280b203d))
- **website:** Add a code signing status page ([18bcf3b](https://github.com/StringKe/kxen/commit/18bcf3b37072c47b4263ad5f7f1c4c2dceb1808a))
- Finalize all documentation for the web mode release ([a272c7f](https://github.com/StringKe/kxen/commit/a272c7f49ac1fa3ee9518b2fa87662af1809173d))
- Align the 0.1.0 changelog heading with the release validator ([652b5b4](https://github.com/StringKe/kxen/commit/652b5b4450cffb345972f4bb766d12e6b82dad78))
- **readme:** Keep the product pitch, defer install detail to the website ([5782b93](https://github.com/StringKe/kxen/commit/5782b939cab66cf001ec18d9aa8970855c2d462c))

### 新增功能

- **web:** Adapt the frontend for plain-browser use ([b8f7f6b](https://github.com/StringKe/kxen/commit/b8f7f6b7bb9de290990be469d3d3eb603b819bb4))
- **cli:** Add kxen-web headless server binary ([15fa181](https://github.com/StringKe/kxen/commit/15fa181b8c4a7d8c5dd15bbe2b166e6203f9e952))
- **tray:** Add system tray as the GUI/web switchboard ([58a08ac](https://github.com/StringKe/kxen/commit/58a08ac1d1571c0b9664d46ccce371d9d3cc9714))
- **release:** Publish multi-arch GHCR image for the kxen server ([0d29fca](https://github.com/StringKe/kxen/commit/0d29fcaf9d693bd0b4c9057fe8bea9048e94bb73))

### 架构与重构

- **server:** Extract core into lib and serve a single axum /ws endpoint ([559c4c6](https://github.com/StringKe/kxen/commit/559c4c66a9639df63139e8b1e82f16cea191ea2f))
- Rename kxen-web to kxen and the shell crate to kxen-gui ([5c48211](https://github.com/StringKe/kxen/commit/5c48211fcd19414b1dbdb3eaa824fa967f17bc35))
- Move core crates out of src-tauri to workspace root ([6ad8822](https://github.com/StringKe/kxen/commit/6ad8822490424a364dadeb995daca172309e14b4))

### 维护

- Prune low-value comments and external product references ([9f87247](https://github.com/StringKe/kxen/commit/9f87247390dddb2229a695f4f31936246d0d1cd7))
- Ignore local docs/ working notes ([3bb761d](https://github.com/StringKe/kxen/commit/3bb761da1efced385a09e22fa455b55a7485963b))

### 问题修复

- **assets:** Use the actual app icon for the in-app hero image ([38473cd](https://github.com/StringKe/kxen/commit/38473cdf88e16ff98ee60bb08b9694d42d9da4d9))
- **platform:** Gate macOS-only dependencies for cross-platform builds ([78e94e4](https://github.com/StringKe/kxen/commit/78e94e42044755b8a1de0fc16223339a49610268))
- Scope the docs/ ignore to the repo root ([cdbb6cd](https://github.com/StringKe/kxen/commit/cdbb6cde9b6f1d77d97d073d37415182d24b6af1))
- **ci:** Green the ubuntu and windows rust jobs ([4dbb512](https://github.com/StringKe/kxen/commit/4dbb512343056574fabcf09eaf9c288b87b4d573))
- **build:** Gate process_group to unix for the windows compile ([e365625](https://github.com/StringKe/kxen/commit/e365625c8ea2dcb3cadf4b4d27a1b1f545458bb8))
- **goal:** Tolerate concurrent removal in list_checked ([a1d1d78](https://github.com/StringKe/kxen/commit/a1d1d78f034b5fc52ef5b7a9b76386e569141955))
- **build:** Clear windows-only clippy warnings ([b24e876](https://github.com/StringKe/kxen/commit/b24e876b7376aeb4cb4c17e13782c17e62d95d07))
- **build:** Split path_policy tests and gate unix-only test helpers ([b8bf1a6](https://github.com/StringKe/kxen/commit/b8bf1a63e63f3bc9b9860526438bd55e19848ae5))
- **release:** Build the renamed kxen-cli package in the release matrix ([fb9e55a](https://github.com/StringKe/kxen/commit/fb9e55a10bdd1886c71e85cf98e15c426c89c027))

## [0.0.1] - 2026-08-07

### 新增功能

- Kxen v0.0.1 development preview ([4948e86](https://github.com/StringKe/kxen/commit/4948e8680cd4674f951e21741bf9b8e7a66b8f6e))
