# P0 plan：composer 全家桶 + OKF 闭环 + 状态栏/设置/统计（终稿）

- **Status**: CONFIRMED-SCOPE（调研归纳完成，待开工）
- **Date**: 2026-07-21
- **证据底座**: `docs/research/2026-07-21-agent-ux.md`（6 路并发调研，全部带一手来源）
- **前置分析**: 能力地图六域盘点 + Cursor/Zed/Windsurf/LobeHub/VS Code agent sessions 布局调研

## 0. 修掉的存量 bug

- 草稿态点新会话不清空时间线（createEffect 对 "" 直接 return）
- model picker 从顶栏右上移到 composer action bar（四家产品全在 composer，零家在顶栏）
- 新会话反馈：空态 + composer 自动聚焦 + 侧栏草稿高亮

## 1. Composer 全家桶

### 1.1 结构

```
[chips 行：@文件 / @Web / 图片缩略图]
textarea（@ 触发补全弹窗 / / 触发命令弹窗 / # 触发知识快捷写）
action bar：左 [+附件菜单][模式预留]   右 [预估 tokens][模型 pill▼][发送/停止]
```

### 1.2 @ 引用（调研结论数值）

- 触发：裸 `@`，光标前最右一个，前界行首/空白/`([{`，后界非空白；补全弹窗 200ms 防抖、上下键移动、Enter/Tab 确认、Esc 关闭、chip 尾部 Backspace 整块删
- chip：惰性解析 token，数据结构 `Chip = { id, kind: file|dir|web|docs|image, ref, label }`；同名文件标签升级末两级路径
- 注入（Cline 式 XML）：正文改写 `'path' (see below for file content)`，尾部 `<file_content path="...">` 按序拼接去重
- 大文件降级：16KB 转符号大纲 `[L12-45]`；硬 cap 单文件 64KB / 单消息 200KB
- @Web：webfetch 现成，`<url_content url="...">`；@Docs 本期对齐 OKF 知识库检索

### 1.3 / 命令 + skill 双触发

- 弹窗统一入口：内置命令（write-goal/doctor/clear/model/abort）+ .kxen/commands/*.md + skills（标注类型徽标）
- SKILL.md 规范：开放标准 `name`/`description` 必填 + 扩展子集 `when_to_use`/`arguments`/`disable-model-invocation`/`user-invocable`；清单注入 description 截断 250 字符
- 目录：项目 `.kxen/skills/` + 用户 `~/.kxen/skills/` + 兼容 `.agents/skills/`；扁平 .md 与目录型并存；深度 cap 8；同名 first-wins
- 双触发统一包装 `<kxen-skill-loaded name trigger dir args>` 持久化为用户消息；递归 cap 3；"同 args 已加载禁止重调"
- 参数展开：`$ARGUMENTS`/`$N`/`$name`，无占位符尾部追加 `ARGUMENTS: <raw>`
- commands/*.md：文件名=命令名，模板 `$ARGUMENTS` 替换后作为消息发送

### 1.4 abort

- CancellationToken（AtomicBool 升级）：stream 段 select 取消 + 工具执行段每步检查
- 统一清扫：进行中的 tool call 落库 interrupted 终态，assistant finish_reason=aborted，**禁止自动重试**
- 子代理独立 token，父 token 级联；RPC `session.abort {session_id}`；发送键 streaming 时变停止键

### 1.5 图片（本轮数据结构 + 交互，vision 发送 P1 收尾）

- `Message.content: Vec<Part>`，`Part = Text | Image{media_type, data(base64)}`（全链路只 base64）
- 预处理：长边 <=1568px、>5MB 转 JPEG q85、WebP/GIF 给 xAI 前转 PNG
- chip 缩略图整块选中/删除，单图 10MB / 单消息 20 张
- capability 驱动：模型注册表加 `image_in`；kimi-for-coding 默认 false
- 降级：无 image_in 模型时用订阅内首个 vision 模型生成 <=200 字描述注入；fallback 不可用明确报错

## 2. OKF 闭环

### 2.1 读侧补齐

- skills 索引进 system prompt（250 字符截断）
- globs 动态激活：rule frontmatter globs 命中 @chip 文件/会话涉及文件才注入
- 多层就近：@chip 文件向上目录链的 AGENTS.md/.agents 优先于根
- mid-turn 刷新：system message 每 loop turn 重建（goal/OKF 动态段）

### 2.2 写侧（知识沉淀，P1 前半）

- 双库：`.agents/rules/*.md`（用户写，入 git）+ `.kxen/memory/`（agent 写，gitignored）
- MEMORY.md 索引 + topic 文件；索引 cap 200 行/25KB，超限返回 error 强制重写收敛
- 三通道：composer `#` 前缀快捷写（弹层选 project/global）+ 会话内 agent 写 + `/done` smol 角色 distillation 出 diff 用户确认
- 不做 silent auto-write；无 TTL；doctor 定期提议 trim；设置页可逐条审/编/删

## 3. 状态栏 v1（底部单行）

- 固定段+开关：`[statusline] items = ["workdir","git","goal","tasks","ctx","tokens","model"]` 每段 bool
- 内容：左 workdir+分支 / 中 goal 徽标+任务计数 / 右 ctx%（10 格，<70 绿 70-89 黄 >=90 红）+ 会话 tokens + 模型
- 单行 cap 120 列；300ms debounce + 事件驱动；git 段 5s 缓存

## 4. 设置页骨架

- 左导航 5 区：通用（主题/字体）-> 模型路由 -> 用量与统计 -> 知识库 OKF -> 高级（hooks/limits）
- 模型路由：role/provider/model/fallback 四列表格（chat/thinking/planning/execution/review/research + default）；mrm 静态兜底链 config 化 `roles.<role>.fallback`
- merged view 展示、写入只落用户层 config.toml；doctor 内嵌进高级区

## 5. 消息统计

- footer 徽标 `1.2k tok · 28 tok/s`，hover 详情 {prompt/completion/ttft_ms/duration_ms/model}
- stream 计时：首 Delta 时间戳 - 发送时间戳 = TTFT；tok/s = completion / 生成时长
- composer 右下预估 input tokens（>80% 橙 / >95% 红）
- edit 工具卡展开内联 shiki diff（edit 结果已含 diff 字段）

## 6. checkpoint（P1 前半，本轮只落数据设计）

- shadow git：bare repo 存 `~/Library/Application Support/kxen/snapshots/<project_hash>`，--git-dir+--work-tree
- 文件编辑工具成功后 commit，粒度绑 messageID；三档 restore + unrevert
- cap：每 session 100 滚动、30 天清理、单文件 2MB、排除 node_modules/target；bash 改动不追踪

## 7. Agent Teams（P0，对标 Claude Code agent teams + kxen 差异化）

### 7.1 模型

- team = lead（主会话）+ teammates（独立 AgentContext + 常驻 tokio loop：spawn -> 首轮 -> idle 听 inbox -> 唤醒再跑）
- **差异化：每个 teammate 可绑不同订阅/模型**（spawn model 参数 -> MRM 解析；默认走角色路由）——多订阅混用主场
- 存储 `data_dir/teams/<session_id>/`：config.json（members/状态，session 结束清理）+ tasks.json（保留）+ inboxes/<name>.json
- 无嵌套 team；单 session 单 team；lead 固定

### 7.2 协调机制

- tasks.json：{id, title, status: pending|in_progress|completed, assignee?, depends_on[]}；依赖完成自动解锁；进程内 Mutex 串行化 claim（单进程无需文件锁）
- mailbox：inboxes/<name>.json 追加写 + 读取时校验（坏行报错剔除）；bus 事件自动送达不轮询
- plan 审批：spawn plan_approval -> teammate 只读跑出计划 -> lead approve/reject（reject 带反馈继续改）
- hook 挂点：teammate_idle / task_created / task_completed（复用 hooks 体系，exit 2 = 打回）

### 7.3 工具面

- lead：`team {action: spawn|message|shutdown|list}`，spawn 参数 {name, role, prompt, model?, plan_approval?, worktree?}
- teammate：角色权限白名单工具 + `send_message {to, text}` + `task {action: claim|complete|list}`
- worktree 隔离复用（teammate 各占一个 worktree，零文件冲突）

### 7.4 UI 配套

- team 面板（dock 新区块）：teammates 列表（working 绿呼吸/idle 灰/failed 红状态点 + 模型标签），点击展开其转录（独立消息流视图），内嵌输入框直接对它说话
- 任务列表视图：三态 + 依赖链 + assignee
- teammate 流式：llm.delta payload 加 agent 字段，主会话与 teammate 渲染分流
- 创建入口：自然语言（lead 用 team 工具 spawn），UI 不做硬编码创建按钮（对齐 Claude Code "描述即编排"）

### 7.5 验证（实测）

- 自然语言让 lead spawn 3 个不同模型的 teammates 做并行调研（anthropic/xai/kimi 各一），各自返回结论
- 共享任务列表：3 任务 1 依赖链，依赖完成自动解锁并被 claim
- 直接对某个 teammate 发消息（UI 面板输入框），其转录更新且主会话不被打断
- plan 审批：spawn plan_approval teammate，其改动前停在计划态，lead approve 后才动手
- teammate_idle hook 配置 exit 2 打回一次生效

## 验证（全部实测）

- cargo test 全绿、vp check 全绿、Rust 零警告
- @文件 chip 发送后模型复述文件内容；16KB+ 文件触发大纲降级
- /write-goal 弹窗触发；skill 显式（/skill）与隐式（模型 skill_load）各成功一次
- abort 中断长 run：tool 落 interrupted、无重试、子代理级联
- 状态栏三段显示 + ctx 阈值变色；设置页改 execution 角色模型后 agent 派发实际走新模型
- mid-turn：goal 状态变化在下一轮 LLM 调用可见
