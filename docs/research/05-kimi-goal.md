# Kimi Code Goal 机制理解

- 整理日期: 2026-07-20
- 一手来源: 本会话即运行在 kimi-code 上，内置 `write-goal` skill 与 goal 工具集（CreateGoal / GetGoal / UpdateGoal / SetGoalBudget）的行为约定直接可见
- 产品侧来源: https://www.kimi.com/code/docs/en/ 、 https://github.com/MoonshotAI/kimi-code

## 1. Goal 生命周期（来自 kimi-code 内置实现）

核心语义：

- Goal 是「跨多轮持续追求的持久目标」，必须有可验证的完成条件（completion criterion）；问候、普通问答、模糊意图不允许建 goal
- 创建：用户显式要求或被要求自治工作时创建；意图模糊时先问清完成判据再建；用户坚持模糊意图时尊重并创建
- 状态机: active -> complete / blocked（另有 pause / resume 入口）
- 每个 goal turn 做一片实质工作；只有目标达成且验证通过才允许 complete；预算将尽或「想停」不允许 complete
- blocked 规则（重要设计）：
  - 目标本身不可能 / 不安全 / 矛盾 -> 当轮即可 blocked
  - 非终态阻塞（缺凭据、外部条件、持续技术失败）-> 同一阻塞条件需连续出现至少 3 个 goal turn 才允许 blocked，防止过早放弃
- 预算：支持 turns / tokens / 时间（秒到 24 小时）三类硬上限，只按用户明确给出的额度设置，不虚构
- 禁止 parallel goal：同一时刻只有一个活跃 goal；replace 需用户明确放弃旧目标

`/goal` 写法（write-goal skill 的约定）：把粗糙意图变成「完成契约」，包含终点线（finish line）、证据（proof）、边界（boundaries）、停止规则（stop rule）。

## 2. 与 sub-agent 调度的联动（Kimi Code 侧观察）

- Kimi Code CLI 内置 `coder` / `explore` / `plan` 三个 subagent，在隔离上下文里派发，保持主会话干净（来源: MoonshotAI/kimi-code README）
- 会员体系里有独立的 Agent Swarm（beta）能力：按档位限制并发子任务数（2 / 4 / 4 / 8）与使用次数（25 / 50 / 120 / 240 次每月）（来源: https://www.kimi.com/help/membership/membership-pricing ）
- goal turn 串行推进 + subagent 并行执行是两层：goal 负责「什么时候算完」，subagent 负责「怎么更快做完」

## 3. kxen 的 Goal 设计要点（吸收 + 扩展）

直接吸收：

- completion contract：objective + 可验证完成条件，缺了不许建
- 状态机与 blocked 三次规则
- 三类预算（turns / tokens / 时间）
- 单活跃 goal + 显式 replace

kxen 扩展：

- Goal 增加 queue：多个 goal 排队，按优先级逐个激活（kimi-code 当前是单 goal 语义，queue 是 kxen 的新增）
- Goal 预算与全局 Model Resource Manager 打通：goal 级 token / agent 预算是全局预算的子账户
- Goal 的执行单元可以是 Dynamic Workflow：goal 引擎每轮决定「直接干 / 派 subagent / 生成 workflow」，workflow 结果回流给 goal 做完成验证
- 验证工具化：completion criterion 尽量落到可执行检查（测试通过、命令输出、文件存在），而不是模型自评

## 4. 与 Claude Dynamic Workflow 的分工

- Goal = 持久意图 + 完成判据 + 预算 + 状态机（回答 why / when done）
- Workflow = 一次性可保存的编排脚本（回答 how / at what scale）
- 二者不互相替代：没有 goal 的 workflow 是单次自动化，没有 workflow 的 goal 在超大任务上会把主上下文打满
