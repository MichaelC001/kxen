# 贡献指南

## 开发环境

- 开发系统： macOS 14 或更高版本、Linux 或 Windows。CI 在三个桌面平台运行 Rust fmt、check 和 clippy，测试在 macOS 和 Ubuntu 运行，Windows 暂不跑测试；本地发布脚本 `scripts/local-release.sh` 只覆盖 macOS arm64。
- Node.js 22.12 或更高版本。
- pnpm 11.15.1。
- 当前 stable Rust toolchain。
- Google Chrome、Chromium 或 Microsoft Edge，仅在开发或验证 Browser automation 时需要。

安装应用依赖和本地门禁工具:

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm exec playwright install webkit
rustup component add llvm-tools-preview
cargo install --locked cargo-llvm-cov --version 0.8.7
cargo install --locked cargo-audit --version 0.22.2
cargo install --locked git-cliff --version 2.13.1
```

启动桌面应用:

```bash
pnpm tauri:dev
```

## 变更流程

1. 从最新 main 创建分支。
2. 涉及多个模块的变更，先在 Issue、Pull Request 描述或双方确认的任务计划中明确范围、风险和验证方式。
3. 保持 Workspace 和 Session 隔离，不引入跨 Session 的全局运行时状态。
4. 提交前运行全部门禁。
5. 使用 Conventional Commits，格式为 `<type>(scope): <desc>`。

## 必须通过的门禁

```bash
pnpm check
pnpm typecheck
pnpm test
pnpm coverage
pnpm build
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo test --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/rust-coverage.sh
pnpm audit --prod --audit-level high
cargo audit --file Cargo.lock
```

`pnpm check` 已包含 `pnpm typecheck`，CI 使用同一入口；单列命令用于本地快速复现 TypeScript strict 类型错误。

仓库根是 Cargo workspace，包含 `crates/kxen-core`（全部产品逻辑，lib `kxen_core`）、`crates/kxen-cli`（无头 server bin `kxen`）和 `src-tauri`（`kxen-gui` Tauri 桌面壳 crate），cargo 门禁必须使用 `--workspace` 覆盖三者。rust-embed 在编译期读取仓库根 `dist/`,cargo 命令前必须先执行 `pnpm build`，否则 Rust 编译失败。

官网变更还必须运行:

```bash
cd website
pnpm install --frozen-lockfile
pnpm check
pnpm audit --prod --audit-level high
```

## Pull Request

Pull Request 需要说明问题、实现边界和验证结果。不要提交 `.env`、证书、私钥、API key、coverage 输出或构建产物。

## 发布流程

发布版本必须同时更新以下版本来源，版本号均不带 `v` 前缀:

- `crates/kxen-core/Cargo.toml` 的 `package.version`。
- `crates/kxen-cli/Cargo.toml` 的 `package.version`。
- `src-tauri/Cargo.toml` 的 `package.version`。
- `Cargo.lock` 中 `kxen-core`、`kxen-cli` 和 `kxen-gui` package 的 `version`。
- `src-tauri/tauri.conf.json` 的 `version`。

`CHANGELOG.md` 不手工编辑。完成版本号修改和本版本功能提交后，运行:

```bash
scripts/changelog.sh generate vx.y.z
scripts/changelog.sh check vx.y.z
```

该脚本使用固定版本的 `git-cliff`、`cliff.toml`、Git tag 和 Conventional Commits 生成完整历史。将生成文件与版本号修改一起放入 `chore(release): prepare vx.y.z` commit；`chore(release)` 不进入最终 Changelog，commit 后再次运行 `check` 必须为 `PASS`。

在版本 commit 已进入 `main` 后创建并推送稳定版 SemVer tag，例如 `v0.2.0`。当前更新通道不接受 prerelease 或 build metadata tag，避免 prerelease 进入稳定版 `latest.json`。不要从尚未进入 `main` 的分支 commit 创建发布 tag。推送 tag 后，在 GitHub Actions 中从 `main` 手动运行 `Release`，并输入该 tag。tag push 不会自动访问发布凭据，避免执行 tag commit 中的 workflow 定义。`.github/workflows/release.yml` 会依次执行:

1. 从可信 `main` 固定 workflow 和校验器，校验 tag 格式与祖先关系，确认 tag commit 已进入远端 `main` 后才 checkout 目标代码。checkout 后仍执行已固定的校验器，并检查 checkout commit 与 tag 一致。
2. 检查上述版本来源、生成后的 changelog 和 Tauri updater 配置一致。
3. 对同一个不可变 commit 重新运行 frontend、Rust 和官网的完整 CI 门禁。
4. 按发布矩阵在六个平台的 runner 上构建: macOS(arm64、x86_64）经 Developer ID 签名和 Apple 公证（桌面 App、DMG 和 `kxen` CLI 同一链路）,Linux(x86_64、arm64）产出 AppImage 和 deb,Windows(x86_64、arm64）产出 NSIS 安装包，同时构建各平台 `kxen` 无头 server 包并逐平台验证产物。发布矩阵的平台、runner、rust target 和稳定 asset 命名以 `scripts/release-manifest.sh` 为单一出处，`scripts/release-manifest.sh json` 可查看实际清单。
5. 使用固定的 `git-cliff` 2.13.1 和 tag 区间生成仅属于当前版本的 Release Notes；同一内容写入 `latest.json.notes`，并作为 GitHub Release body 的变更部分。
6. 合并各平台 updater 签名生成并校验 `latest.json` 与 `SHA256SUMS`，将各平台验证过的 release 文件作为 workflow artifact 传递给独立 publish job。
7. publish job 只接收已验证 artifact，不接收签名凭据。它先创建 draft，重新下载并逐字节核对全部远端 asset，全部一致后才公开 release。

Release environment 必须配置 `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_TEAM_ID`、`APPLE_API_ISSUER`、`APPLE_API_KEY`、`APPLE_API_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。这些值必须是 environment secret，不是 repository secret。`release` environment 的 deployment branch policy 必须只允许 `main`，仓库 tag ruleset 必须允许创建 `v*` 但禁止更新和删除已有发布 tag，仓库必须开启 GitHub Immutable Releases 以锁定公开 release 的 tag 和 asset。GitHub Actions policy 必须开启 full-length commit SHA pinning，并在不需要所有 action 时将 `allowed_actions` 收紧为经审核的列表。secret 只注入需要它们的单个 step。只有 publish job 具有 `contents: write` 权限。

上述 GitHub environment、ruleset 和 Immutable Releases 是仓库外部设置，workflow 不会自动创建或修改。发布前必须在当前仓库设置中确认；未确认时发布信任链状态为 `UNKNOWN`，已知不满足时为 `FAIL`。

当前仓库外部发布治理状态已核实: `release` environment 的 deployment branch policy 仅允许 `main` 并带有 branch policy protection rule，8 个签名 secret 全部为 environment secret，repository secrets 中已不存在 Tauri updater signing key 与 password；`protect release tags` ruleset 对 `v*` 禁止更新和删除；Actions policy 已开启 `sha_pinning_required`。剩余两项: `allowed_actions` 仍为 `all`，尚未收紧为经审核的列表；GitHub Immutable Releases 无公开 API 可查，状态为 `UNKNOWN`，发布前必须在仓库设置中确认已开启。

工作流拒绝覆盖任何公开 release，也不会删除人工创建的 draft。publish job 失败时只清理当前 run 拥有的未完成 draft；清理遇到临时故障时，下一次同 tag run 会识别并删除旧的 workflow-owned draft，再从已验证 artifact 重新创建。公开 release 始终保持不可覆盖。

## macOS 发布冒烟检查

macOS 腿的签名和公证链路在每个公开版本发布前必须完成以下检查。自动步骤证明 source gate、签名、公证、updater signature 和 release asset 一致性，真实功能链路仍需在已签名 App 中验证。Windows 和 Linux 暂无等价的自动冒烟脚本，其实机验证状态为 UNKNOWN，发布前需要在对应平台手动确认。

自动检查，在 release runner 或本地已签名产物目录执行:

```bash
bash scripts/verify-macos-release.sh
```

必须全部为 `PASS`:

- `Kxen.app` 的 `codesign --verify --deep --strict`。
- `Kxen.app` 的 Gatekeeper `spctl --assess`。
- `Kxen.app` 的 notarization ticket `xcrun stapler validate`。
- DMG 的 code signature 和 Gatekeeper。挂载后必须只包含一个顶层 `Kxen.app`，其 metadata、notarization ticket 和 CDHash 必须与已验证 build 一致。Tauri 只公证并 staple App；DMG 容器要在 Gatekeeper primary-signature 评估下通过，必须在封装后单独提交公证并 staple ticket。
- updater archive 结构安全且展开大小受限，配套 signature 可由应用配置的 updater public key 验证；解包后的 App 必须通过 codesign、Gatekeeper 和 notarization ticket 校验，且 CDHash 与 build 产物一致。
- `latest.json` 的版本、platform key、signature 和下载 URL 与 tag 和实际 updater artifact 一致。
- `SHA256SUMS` 精确覆盖 DMG、updater archive、signature 和 `latest.json`，且全部校验通过。

已签名 App E2E:

1. 从 GitHub Release 下载 DMG，不使用本地 build 目录的 App。
2. 挂载 DMG，把 Kxen 拖入 `/Applications`，首次启动不应出现「无法验证开发者」。
3. Settings 的首次运行检查显示 Workspace、Provider 和 Routing 均为 `PASS`。
4. 新建 Session，选择一个 Provider，发送消息并确认模型标签与实际 Provider 一致。
5. 选择 Workspace 内文件执行 read/edit，选择 Workspace 外普通文件后可读取；选择 `.p8` 或 kxen 数据目录必须被拒绝。
6. 执行 Shell 命令，必须先出现包含完整 command 和 cwd 的宿主机 Approval；拒绝后不得执行。
7. 创建 Goal、Schedule、Workflow 和 Team，并确认各入口可发现、状态可回放。
8. 分别执行「直接删除」和「删除并沉淀个人知识」；后者不得写项目 `.agents/`。
9. Browser automation、Remote MCP、自动知识沉淀在全新配置中必须为关闭。
10. 用上一公开版本检查更新，必须读取 GitHub Release 的 `latest.json`，签名验证通过后完成安装和重启。

任何一项未验证均记录为 `UNKNOWN`，不得写成 `PASS`。
