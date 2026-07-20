# 上游同步流程（anomalyco/opencode -> kxen）

- 版本: 1.0
- 日期: 2026-07-20
- 依据: docs/plan/01 第 5 节；OMP porting 模式（sync-point + format-patch + scope 替换 + protected-features）

## 拓扑

```
anomalyco/opencode   上游（只读跟踪）
StringKe/opencode    fork（同步桥梁，不开发）
StringKe/kxen        主仓库（全部开发在此）
```

- fork 本地 clone: `/Users/xiaobai/Code/SelfCode/opencode`（仅同步用）
- sync-point 记录: kxen 仓库根 `SYNC` 文件的 `upstream-sha`

## 同步步骤

1. fork 追上上游:

   ```bash
   gh repo sync StringKe/opencode
   # 或本地: cd /Users/xiaobai/Code/SelfCode/opencode && git fetch upstream dev（无 upstream remote 时先 git remote add upstream https://github.com/anomalyco/opencode.git）
   ```

2. 生成增量 patch（限保留包路径，删过的包直接排除）:

   ```bash
   cd /Users/xiaobai/Code/SelfCode/opencode
   git fetch --deepen=200 origin dev   # 浅 clone 需要足够历史
   OLD=<SYNC 里的 upstream-sha>
   NEW=$(git rev-parse upstream/dev)
   EXCLUDES=(
     ':(exclude)packages/tui' ':(exclude)packages/cli' ':(exclude)packages/desktop'
     ':(exclude)packages/web' ':(exclude)packages/console' ':(exclude)packages/stats'
     ':(exclude)packages/enterprise' ':(exclude)packages/slack' ':(exclude)packages/function'
     ':(exclude)packages/storybook' ':(exclude)packages/docs' ':(exclude)packages/containers'
     ':(exclude)packages/identity' ':(exclude)packages/opencode/src/acp'
     ':(exclude)packages/opencode/src/ide' ':(exclude)packages/opencode/src/share'
     ':(exclude)packages/opencode/test/acp' ':(exclude)packages/opencode/test/ide'
     ':(exclude)packages/opencode/test/share'
   )
   git format-patch $OLD..$NEW --stdout \
     -- packages script sdks bunfig.toml turbo.json tsconfig.json .oxlintrc.json "${EXCLUDES[@]}" \
     > /tmp/upstream.patch
   ```

3. scope 替换（@opencode-ai/ -> @kxen/ 等）:

   ```bash
   bun run /Users/xiaobai/Code/SelfCode/kxen/script/sync-scope.ts < /tmp/upstream.patch > /tmp/kxen.patch
   ```

4. 套用与冲突处理:

   ```bash
   cd /Users/xiaobai/Code/SelfCode/kxen
   git apply --check /tmp/kxen.patch        # 先检查
   git apply --3way /tmp/kxen.patch         # 三方合并尽量自动
   # 冲突时对照 protected-features 清单人工取舍（见下）
   ```

5. 验证与落点:

   ```bash
   bun install && bun run typecheck && bun turbo test
   ```

   全绿后更新 `SYNC` 的 `upstream-sha = $NEW`，连同 patch 应用结果一起提交（message 注明 upstream 区间）。

## protected-features（上游 patch 不得覆盖）

冲突时这些位置以 kxen 为准：

- packages/kxen-*（safety/mrm/subagent/agents/goal/workflow，全部 kxen 自有）
- packages/opencode/src/auth/import.ts、src/plugin/anthropic/、src/kxen/、src/cli/cmd/{start,stop,doctor}.ts、src/tool/{workflow,exec}.ts、src/index.ts（命令面）
- packages/opencode/src/session/llm.ts 的 mrm 挂载段、src/agent/agent.ts 的角色注入段、src/config/config.ts 的 share 强制段
- packages/core/src/system-context/{agents,capabilities}.ts、src/flag/flag.ts 的环境归一化段、src/global.ts 的 app 名、src/git.ts 的 LC_ALL、core/src/location-services.ts 的 kxen node 挂载
- packages/session-ui/src/components/markdown-mermaid.ts 及 markdown.tsx 的 mermaid 挂载
- 根: SYNC、docs/、AGENTS.md、README.md、script/sync-scope.ts、.mise.toml、turbo.json 的 kxen#test
- 已删除清单（packages/tui、cli、desktop、web、console、stats、enterprise、slack、function、storybook、docs、containers、acp/ide/share 子系统）——上游对这些的更新直接忽略

## 分叉清单（明确不跟上游）

- TUI / Desktop / 官网 / 云端 console：已物理删除，上游更新忽略
- share 子系统：kxen 已删，上游相关更新忽略
- Anthropic/Kimi 订阅接入：kxen 自有实现（src/auth/import.ts + plugin/anthropic/claude.ts），不期待上游补
