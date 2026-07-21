# kxen 产品需求文档（PRD）

- 版本: 2.0
- 日期: 2026-07-21
- 状态: 与 docs/rust/01-design.md（v3.0）对齐

## 1. 产品概述

- 产品名: kxen
- 域名: https://kxen.ai （已注册）

kxen 是 macOS Apple Silicon 专精的开源 Coding Agent Harness，**只做 coding 场景**。它综合当前所有开源 agent-cli 的优点：Claude Code 的 Dynamic Workflow 与 sub-agents、Kimi Code 的 Goal 生命周期、grok-build 的命令调度速度、jcode 的性能、peri 的缓存命中、OpenCode 的 provider 广度、pi_agent_rust 的轻量工程——以 Tauri 纯 GUI 单 app（Rust + WKWebView）交付。

面向已经持有多个模型订阅的重度用户：一个 app 混用全部订阅与 provider，可调度、可控制、可魔改。

## 2. 核心目标

- 综合所有开源 agent-cli 的优点于一个工具（优点收纳矩阵见设计文档附录 A）
- 性能对齐 jcode 实测基准：安装包 < 20MB、常态内存 < 80MB、首绘 < 500ms
- 模型调度是全局一等公民：角色化路由 + 并发/限额/降级，作用于一切调用
- 编排是一等公民：Dynamic Workflow（模型自主写脚本编排）与 Goal（持久生命周期）同体
- 安全在 execution 层：灾难操作硬拦截，不做内容级风控

## 3. 关键需求

### 3.1 形态与平台

- Tauri 2.x 纯 GUI 单 app，仅 macOS Apple Silicon（aarch64-apple-darwin）
- 无 CLI、无 TUI、无 daemon、无端口、无 HTTP server
- doctor 为 GUI 状态页；upgrade 走 tauri-plugin-updater（GitHub Releases）

### 3.2 模型与订阅

- provider 全通用：Rig 20+ 家 + openai-compatible，不特殊化任何一家
- 订阅接入 = 通用「官方 CLI 凭证探测」机制：读官方 CLI 凭证存储、新鲜度优先、过期自动刷新；新增订阅 = 加一条探测规则
- 当前四条规则：Claude（Keychain）、Codex（~/.codex）、Grok（~/.grok）、Kimi（~/.kimi-code）
- 角色化模型路由（thinking/planning/execution/review/research）+ mrm 全局调度（并发/RPM/降级链/状态注入）

### 3.3 编排

- Dynamic Workflow：模型自主写 JS（rquickjs 沙箱执行），agent()/pipeline()/constraints()/phase() 原语，中间结果不进主上下文，缓存恢复，200 调用护栏
- Goal 生命周期：完成契约必填、预算三维、阻塞三次规则、score-based 逐条验证、write-goal 引导起草（AskUserQuestion 驱动）
- 角色化 subagent：预置 model/permission/prompt，task 派发
- loop 检测四层（exact/semantic/stagnation/churn），防空转

### 3.4 命令与工具

- 命令调度（grok-build 实证）：auto_bg 15s 自动后台化、完成通知代替 sleep/poll、任务三件套、静态快照 shell、命令遮蔽（grep->ugrep/find->bfs）
- dev server 管理：就绪等待（pattern/端口）、restart_task、list_tasks、健康检查、GUI 任务页
- exec(type: zsh|bash|fish, path, command)：type 必填 + 方言校验
- 读写删：LINE#HASH 锚点（ChunkFingerprint）、锚点/兼容双模式 edit、免强制 read-before-edit、find_shifted 自愈、rm->trash 遮蔽（删除可恢复）
- safety F1-F5 执行层硬拦截（毁系统/毁目录/删 .git/基础设施毁灭/批量失控），无内容级风控
- 渐进披露（Tool Search）：常驻 ~12 工具，其余按需发现，保 prompt cache 命中

### 3.5 上下文工程

- frozen/dynamic 分段 + boundary marker，保 provider prompt cache 命中
- .agents/ OKF：rules 注入型 + references 按需 + index.md 渐进披露 + 多层目录就近
- 会话 JSONL 持久化 + branch/fork/resume + LLM 摘要 compaction

### 3.6 性能与安全

- 无 Clone 原则：hot path 零分配、Arc<str> 共享、RegexSet 预编译、HTTP client 单例、事件零拷贝
- release 全 LTO；目标：包 < 20MB、内存 < 80MB、首绘 < 500ms、首 token < 2s
- Tauri capabilities 最小授权；凭证只读；零遥测零上传

## 4. 非目标

- Windows / Linux / Intel Mac
- CLI / TUI / daemon / HTTP API / 插件市场 / 移动端
- 内容级提示词风控、沙箱（safety 拦截代替）
- 遥测 / 会话分享 / 云同步

## 5. 成功标准

- 四订阅各完成一次真实调用（模型名单实拉），provider 列表无裁剪
- Dynamic Workflow 真实编排跑通（主 agent -> workflow -> 子 agent -> 结果）
- goal 全生命周期流转可演示（含预算拦截与阻塞三次规则）
- dev server 起停/就绪/重启/崩溃感知全链路可演示
- `rm -rf /` 被 safety 拦截；`rm file` 实际进回收站
- release dmg < 20MB；常态内存 < 80MB；首绘 < 500ms

## 6. 开放问题

- Rig 对 codex 订阅端点的适配度（不合则 codex 单独自写 ~200 行）
- rquickjs 的 tokio 桥接形态（M4 首个技术验证点）
- updater 发布管线细节
