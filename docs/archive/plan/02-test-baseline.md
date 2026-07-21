# 测试基线（M2 跑通时记录，2026-07-20）

基线命令: `bun turbo test --concurrency=4`
用途: goal proof 第 4 条的对照基准（保留包测试通过率不劣于此基线）。

## 基线数据

| 包 | 结果 |
| --- | --- |
| packages/ui | 9 pass / 0 fail |
| packages/session-ui | 75 pass / 0 fail |
| packages/app | 656 pass / 0 fail |
| packages/core | 1074 pass / 0 fail |
| packages/opencode | 3178+ pass / 0 fail（见已知 flake） |

全量合计约 4992 pass，0 稳定失败。

## 已知 flake（不计入失败）

- `packages/opencode test/server/httpapi-v2-pty.test.ts` 的 `serves location-wrapped PTY routes and retains exited sessions`：全量并发跑时偶发超时（约 5.2s 边界），单跑 3 次稳定通过。上游测试的并发稳定性问题，非 kxen 改动引入。

## 迁移期修复记录（影响基线的三次修复）

1. app tsconfig 补 `types: ["vite/client", "bun"]`（typecheck 红 -> 绿）
2. i18n parity：摘 desktop domain、17 locale 补齐缺失 key（英文 fallback）、清残留 key
3. git worktree 调用强制 `LC_ALL=C`（dirty worktree 的 force 检测不再受用户 git locale 影响）
4. help-text 快照测试固定 `LC_ALL: "C"`（快照 locale 无关化）
