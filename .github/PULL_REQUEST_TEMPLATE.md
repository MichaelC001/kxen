## 变更说明

<!-- 解决了什么问题？关联 Issue 用 Closes #123 -->

## 实现边界

<!-- 改了哪些模块？明确没有改什么（避免范围蔓延） -->

## 验证证据

<!-- 勾掉不适用项，贴上关键输出。没有验证证据不要标记完成 -->

- [ ] `pnpm check`
- [ ] `pnpm test`
- [ ] `pnpm build`
- [ ] `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features`
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`
- [ ] 涉及官网：`cd website && pnpm check`

## 检查清单

- [ ] 提交信息遵循 Conventional Commits
- [ ] 未提交 `.env`、证书、私钥、API key、coverage 输出或构建产物
- [ ] 新增 RPC 已同步 handler / request_schema / 前端调用三方
- [ ] 无法验证的项已在描述中标注 UNKNOWN
