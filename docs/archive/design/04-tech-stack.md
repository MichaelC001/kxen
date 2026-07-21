# 技术选型：Bun + TypeScript

- 日期: 2026-07-20
- 状态: 已定方向（Bun + TS 为主，Rust 按需后置）

## 1. 决策

- 主语言: TypeScript
- 运行时与打包: Bun
- 发布形态: `bun build --compile` 单文件可执行（跨平台）+ npm 包双发
- 性能策略: 第一阶段纯 TS；热点路径预留原生扩展点，按需引入 Rust N-API

## 2. Bun 可行性（已验证，来源: https://bun.com/docs/bundler/executables ）

- `bun build ./cli.ts --compile --outfile kxen` 产出内嵌 Bun runtime 的单文件可执行，支持全部 Bun / Node 内置 API
- 跨平台交叉编译: `--target=bun-linux-x64` / `bun-linux-arm64` / `bun-darwin-arm64` / `bun-darwin-x64` / `bun-windows-x64`，另有 baseline / modern 变体
- `--minify --sourcemap --bytecode` 进一步压缩体积并改善启动（官方明确说编译产物降低内存占用与启动时间）
- 侧证：OMP 生产环境即要求 bun >= 1.3.14 并以 Bun 分发

## 3. 为什么不是 Deno / Node

| 维度 | Bun | Deno | Node |
| --- | --- | --- | --- |
| TS 原生 | 是 | 是 | 需构建 |
| 单文件可执行 | `bun build --compile` 成熟 | `deno compile` 可用 | SEA 繁琐，生态弱 |
| npm 兼容 | 几乎完整 | 已改善但历史包袱仍在 | 原生 |
| 启动速度 | 快 | 快 | 慢于两者 |
| 选型理由 | 生态 + 编译 + 速度最均衡 | 沙箱模型对 coding agent 反而是束缚 | 单文件分发是硬伤 |

## 4. 性能分层策略

第一阶段（纯 TS）：

- 编排逻辑、goal / workflow / subagent / 资源调度全部 TS，无性能顾虑
- 搜索直接用系统 `rg`（先 fork/exec，量小可接受）
- AST 用 tree-sitter 的 WASM binding
- 隔离用 git worktree（纯命令行）

第二阶段（按需，参考 OMP 的 crates 清单）：

- 触发条件：实测 profile 显示工具热路径成为瓶颈（典型信号：大仓库搜索占 turn 时间 >20%，或并发 subagent 时 fork/exec 抖动明显）
- 候选热点：ripgrep 进程内化、tree-sitter 原生、hashline 编辑、隔离后端（APFS clone / overlayfs）、内嵌 shell
- 实现：Rust cdylib + N-API，平台预编译二进制随 npm 包分发（`@kxen/natives-darwin-arm64` 等可选依赖形态）
- 架构预埋：工具层从第一天就走统一接口（`SearchProvider` / `AstProvider` / `IsolationBackend`），TS 实现与原生实现可互换

## 5. 关键依赖

| 依赖 | 用途 | 备注 |
| --- | --- | --- |
| `@earendil-works/pi-ai` | 多 provider 统一调用、usage 统计 | 底座（见 design/01） |
| `@earendil-works/pi-agent-core` | agent loop | 底座 |
| `@earendil-works/pi-coding-agent` | SDK：session / model registry / auth storage | 锁版本跟进 |
| `@earendil-works/pi-tui` 或自研 | TUI | 先评估 pi-tui 是否满足 goal / workflow 视图 |
| YAML 解析 | 配置（models.yml / roles.yml） | 形态参考 OMP |
| tree-sitter (WASM) | 第一阶段 AST | 第二阶段可换原生 |

## 6. 发布管线（已落地，2026-07-20 验证）

- `scripts/build.ts`：`bun run build` 产出当前平台单文件，`bun run build:all` 产出 5 平台矩阵（darwin-arm64 / darwin-x64 / linux-x64 / linux-arm64 / windows-x64），`--compile --minify --bytecode`，版本号经 `--define` 编译期内联
- 已验证：darwin/arm64 本机二进制 61M 可执行（版本注入正确）；linux-x64 交叉编译产出合法 ELF
- 全 Bun 生态无 pnpm：install / workspaces / catalog / test / build / publish 全部 bun；`bun publish` 会把 `catalog:` 引用解析为真实版本号（官方支持）
- npm 包附带 JS 版本（bin: kxen -> cli.ts，shebang 兼容 bun / node）
- install script 形态参考 https://omp.sh 与 https://pi.dev （curl 拉对应平台二进制）
