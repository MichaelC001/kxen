# Changelog

此文件由 [git-cliff](https://github.com/orhun/git-cliff) 根据 Git tag 和 Conventional Commits 自动生成，请勿手动编辑。

## [Unreleased]

### 工程

- **release:** Generate version-specific notes with git-cliff ([15195b2](https://github.com/StringKe/kxen/commit/15195b264733666d851794915d573a911d09aa55))

### 文档

- **release:** Document generated changelog workflow ([38233ca](https://github.com/StringKe/kxen/commit/38233ca5e0a97c8b2c94c2f68e5ee1e95618589d))

### 问题修复

- **release:** Normalize generated changelog output ([9f5ac0b](https://github.com/StringKe/kxen/commit/9f5ac0b7394902fbf3b58f6c56fe26555a71c4b7))

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
