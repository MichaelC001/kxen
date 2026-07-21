# write-goal：把模糊意图变成可自动推进的 goal

（借鉴自 MoonshotAI/kimi-code 的 write-goal skill（packages/agent-core-v2/src/app/skillCatalog/builtin/write-goal.md），适配 kxen 的 goal 工具与 OKF 体系。）

goal 不是任务描述，是**完成契约**：什么必须变成真、怎么证明、边界在哪、什么时候停下来报告而不是硬磨。

本流程是与用户一起起草 goal 文本。起草与启动是两步：先敲定措辞，用户确认后才用 goal 工具 create + activate。

## 铁律：选择一律走 AskUserQuestion

goal 起草是一连串选择——范围、措辞、预算、权限。每一个都**停下来用 AskUserQuestion**（可 batch 相关选择）。禁止用散文列选项让用户自由回复。仅当 AskUserQuestion 不可用（auto 模式）才退回带标号的短文本。开放式输入（"什么能证明完成？"）可以用散文。

## 好 goal 的形状：proof，不是 effort

- "继续改进代码" 描述 effort，永不结束
- "Done when `bun test` 退出码为 0 且 src/auth 之外无改动" 描述 proof，可检查

契约五要素（按任务规模取舍）：

1. **End state**：必须成真的条件（通过的套件、零匹配的搜索、存在的文件）
2. **Proof**：可观察证据（命令退出码、测试数、glob 为零、指标阈值）
3. **Boundaries**：可以动什么、不许动什么（哪个模块、不删 spec、不动无关文件）
4. **Loop**：迭代方式（每次改动后重跑检查、逐项处理队列）
5. **Stop rule**：完成不可达时如何诚实结束（"停下来报告，不伪造通过"——关乎诚实，不是预算）

两个习惯：让 goal 呈队列形（失败的测试、待迁移文件——有可数定义）；借已有验证（测试、CI、typecheck、零匹配搜索）。

## 预算是 opt-in

**不要默认设预算，也不要把轮数上限写进目标文本。** 良构 goal 自己会停（proof 过或遇阻）。开放式探索任务才建议预算，按 token 成本框，让用户选值。用户要的上限明显过大时指出一次，尊重其最终决定。

## 流程

1. **理解意图**：问用户要的结果和完成证据。缺 finish line 或检查手段，先一起补上。开放问题收敛为具体选项后立刻 AskUserQuestion。
2. **起草**：用用户的语言写目标。复用形状：

   ```
   <什么必须成真。>
   Done when <证明它的命令/搜索/状态>。
   Scope: 只动 <范围>；不 <禁止动作>。
   Loop: <迭代方式>。
   If <阻塞条件>，停下来报告而不是强行通过。
   ```

3. **展示并解释**：全文给用户看，讲清 finish line、proof、边界、停止条件，指出还软的地方。
4. **一起改**：用户反馈 -> 新草案；措辞/范围有多个方向时用 AskUserQuestion 给选择。用户坚持更宽松的写法，指出 trade-off 一次后按用户的写。
5. **创建并激活**：用户确认后，用 goal 工具 `create`（contract: objective + completionCriteria + 可选 constraints/budget），再 `activate`。不要在用户确认前创建。

## 常见错误

- 没问就建议把普通请求包成 goal（普通 "修这个测试" 就是普通请求）
- 用英文起草而用户说中文（跟随用户语言）
- 用户没看全文就启动
- 指定 effort（"继续改进 X"）而不是 proof（"Done when X 过"）
- 没有 blocked 路径（必须写"停下来报告"）
- 目标无法验证（锚到测试/搜索/指标）
