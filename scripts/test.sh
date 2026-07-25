#!/bin/bash
# 分片跑前端测试：vitest 4.1.10 browser 模式单进程跑大套件会在 route GC 处崩溃
# （基建层时序竞态，与断言无关；片越小越稳）。新增测试文件必须加入某个分片。
set -e
cd "$(dirname "$0")/.."

pnpm exec vitest run src/lib
sleep 4
pnpm exec vitest run src/components/composer/text-composer.test.tsx src/components/composer/text-composer-triggers.test.tsx src/components/composer/text-composer-paste.test.tsx src/components/composer/triggers.test.ts src/components/composer/voice-ptt.test.ts
sleep 4
pnpm exec vitest run src/components/composer/attach-menu.test.tsx src/components/composer/attach.test.ts src/components/composer/composer-attachments.test.ts src/components/composer/drag-drop.test.ts src/components/composer/image-scale.test.ts src/components/composer/mic-menu.test.tsx src/components/composer/model-picker.test.tsx
sleep 4
pnpm exec vitest run src/components/settings
sleep 4
pnpm exec vitest run src/components/orb.test src/components/selection.test src/components/AgentFocusView.test src/components/RightColumn.test
sleep 4
pnpm exec vitest run src/components/Dock.test src/components/DockRepoDiff.test src/components/NotificationCenter.test src/components/StatusBar.test
sleep 4
pnpm exec vitest run src/components/SessionRow.test src/components/SessionTree.test src/components/DockWorktree.test src/components/AgentRunCards.test src/components/UserItem.test src/components/AssistantItem.test
sleep 4
pnpm exec vitest run src/pages
