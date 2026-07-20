# Goal 系统与 Dynamic Workflow 设计

- 日期: 2026-07-20
- 状态: 初稿，待评审
- 前置阅读: `docs/research/04-claude-workflows.md`、`docs/research/05-kimi-goal.md`、`docs/design/02-model-routing.md`

## 1. 分工

- Goal：持久意图 + 完成判据 + 预算 + 状态机（回答 why 与 when done）
- Dynamic Workflow：一次性、可保存的编排脚本（回答 how 与 at what scale）
- Subagent：两者的共同执行原语

## 2. Goal 引擎

### 状态机

```
draft -> queued -> active <-> paused
active -> complete | blocked | budget_limited | canceled
blocked / budget_limited -> （用户补充信息后）active
```

规则（吸收 kimi-code 语义）：

- 创建必须有 completion contract：objective + completionCriteria + constraints + budget，缺一不可
- 同一时刻只有一个 active goal，其余 queued（可按优先级排序）
- blocked 采用「三次规则」：非终态阻塞需同一条件连续 3 个执行轮才允许置 blocked；目标不可能 / 不安全 / 矛盾则当轮可置
- 每轮结束跑验证：completionCriteria 尽量落到可执行检查（命令、测试、文件状态），模型自评只作兜底

### 与资源的打通

- goal 预算是全局预算的子账户（perGoal tokens / agents），acquire 时双重记账
- 预算水位 80% / 95% 两档警告；硬顶到 100% 置 budget_limited
- goal 每轮规划时拿到 MRM 状态快照，决定本轮策略：直接干 / 派 subagent / 生成 workflow

## 3. Dynamic Workflow Runtime

### 脚本模型（对齐 Claude，见 research/04）

- 顶层 `await` 的 JavaScript，沙箱执行（无直接 fs / shell）
- API：
  - `agent(prompt, { role, schema, label })` 派一个 subagent，按角色路由模型
  - `pipeline(items, fn, { concurrency })` 逐项 fan-out，并发受 MRM 控制
  - `constraints()` 返回当前资源快照（并发余量 / 冷却中 provider / 配额 / 预算水位）
  - `args` 接收调用方结构化输入
- `meta` 导出名称与描述，可保存为命令（项目级 `.kxen/workflows/` 优先于个人级 `~/.kxen/workflows/`）

### 执行语义

- 后台执行，主会话不被阻塞；中间结果留在脚本变量，只回传最终 return
- agent 级结果缓存：同会话内 resume 时已完成 agent 直接回放，运行中 agent 重跑
- 进度模型：phase 分组（按脚本结构推断或 `phase()` 显式声明），每 phase 统计 agent 数 / token / 耗时
- 规模护栏：默认规模指引 small / medium / large / unrestricted；超过阈值出大 run 警告（只提示不阻断，对齐 Claude）
- 审批：按权限模式决定是否运行前展示脚本摘要（default 问、auto 首次问、bypass 不问）

### 与 Claude 官方的差异

| 维度 | Claude Code | kxen |
| --- | --- | --- |
| 并发 | 固定 16 | MRM 全局动态分配 |
| 总量 | 固定 1000 | 配置 + 预算双重约束 |
| 模型 | 会话模型（可整体覆盖） | 每 `agent()` 按角色多模型混用 |
| 资源感知 | 无 | `constraints()` + 规划时注入 |
| 成本 | 警告 | 警告 + 预算硬顶 |

## 4. 两者联动

典型流程（以「全库审计」为例）：

1. 用户 `/goal 审计 src/ 下所有路由的鉴权缺失，完成判据：报告给出且每条经复核`
2. goal 引擎进入 active，首轮判断任务规模 -> 生成 workflow 脚本（planning 角色模型撰写）
3. workflow 执行：fan-out 审计 agent（execution 角色，优先 Grok / Kimi），再 fan-out 复核 agent（review 角色，优先 Claude）
4. 中间任何 provider 限流 -> MRM 排队 + fallback，脚本无感
5. workflow 返回汇总报告 -> goal 跑 completion 验证 -> 通过则 complete，否则带着差距进入下一轮

## 5. TUI 要求

- goal 视图：状态、contract、预算水位、最近验证结果
- workflow 视图：phase 列表、单 agent 下钻（prompt / 工具调用 / 结果）、暂停 / 恢复 / 停止 / 保存命令
- 资源视图：每 provider 并发占用与冷却状态、角色占用、预算水位
