# 贡献指南

## 开发环境

- macOS 14 或更高版本。
- Apple Silicon。
- Node.js 22.12 或更高版本。
- pnpm 11.15.1。
- 当前 stable Rust toolchain。
- Google Chrome。

安装依赖并启动桌面应用:

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm tauri:dev
```

## 变更流程

1. 从最新 main 创建分支。
2. 涉及多个模块的变更先更新并确认 `plan.md`。
3. 保持 Workspace 和 Session 隔离，不引入跨 Session 的全局运行时状态。
4. 提交前运行全部门禁。
5. 使用 Conventional Commits，格式为 `<type>(scope): <desc>`。

## 必须通过的门禁

```bash
pnpm check
pnpm test
pnpm coverage
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --all-features
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
bash scripts/rust-coverage.sh
pnpm audit --prod --audit-level high
cargo audit --file src-tauri/Cargo.lock
```

官网变更还必须运行:

```bash
cd website
pnpm install --frozen-lockfile
pnpm check
pnpm audit --prod --audit-level high
```

## Pull Request

Pull Request 需要说明问题、实现边界和验证结果。不要提交 `.env`、证书、私钥、API key、coverage 输出或构建产物。
