#!/bin/bash
# 分片跑前端测试：vitest 4.1.10 browser 模式单进程跑大套件会在 route GC 处崩溃
# （基建层时序竞态，与断言无关；片越小越稳）。新增测试文件必须加入某个分片。
set -e
cd "$(dirname "$0")/.."

pnpm exec vitest run src/lib
sleep 4
pnpm exec vitest run src/components/composer
sleep 4
pnpm exec vitest run src/components/settings
sleep 4
pnpm exec vitest run src/components/orb.test src/components/selection.test src/components/AgentFocusView.test src/components/RightColumn.test src/components/Dock.test src/components/DockWorktree.test src/components/NotificationCenter.test
sleep 4
pnpm exec vitest run src/components/SessionRow.test src/components/SessionTree.test src/components/TopAgentBar.test
sleep 4
pnpm exec vitest run src/pages
