# .agents 目录与 AGENTS.md 规范（rules / references / 多层目录）

- 日期: 2026-07-20
- 决策: kxen 的 canonical 配置 = 项目级 `.agents/` 目录 + 根 `AGENTS.md`；文档组织引入 OKF v0.1（https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf ，SPEC: https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md ）
- 前置: design/08 配置互通、analysis/07 prompt 注入层

## 1. 问题定义

AGENTS.md 共识只解决「根文件里写什么」。剩下三个问题没有共识：

- rules（需要注入 prompt 的指令型文档）放哪、怎么路由（全局生效 vs 按路径生效 vs 按需）
- references（API 文档、runbook、规格等知识型文档）放哪、怎么在不烧 context 的前提下可被找到
- 多层目录（monorepo 子项目）里这些文档的继承、覆盖与加载时机

## 2. 引入 OKF 的五个机制

OKF 是 vendor-neutral 的「markdown + YAML frontmatter 目录树」知识格式，kxen 采用其机制而不是其领域词汇：

| OKF 机制 | kxen 用法 |
| --- | --- |
| frontmatter `type` 必填，其余字段自由扩展 | 文档路由的唯一依据；kxen 定义自己的 type 词汇表（见第 3 节） |
| 每层 `index.md` 枚举目录内容（含 description） | 渐进披露入口：模型先读索引再按需读文档，不整树进 context |
| 跨文档 markdown 链接（bundle 相对路径 `/...`） | 文档间引用成图（rule 引用 reference、workflow 引用 skill） |
| 宽松消费（容忍未知 type / 未知字段 / 断链） | 生态兼容：std-agent 等 producer 的输出零转换可读 |
| `log.md` 变更历史（可选） | `.agents/` 的变更可追溯 |

kxen 声明兼容：项目 `.agents/` 即一个 OKF bundle，根 `index.md` frontmatter 里标 `okf_version: "0.1"`。

## 3. 布局

```
AGENTS.md                  # 共识根文件：项目铁律 + 指向 .agents/ 的索引（薄）
.agents/
  index.md                 # 自动生成：全目录索引（禁止手写，kxen sync 生成）
  rules/                   # type: rule，注入型指令
    *.md
  references/              # type: reference，知识型文档（按需读取）
    *.md（可按主题再分子目录）
  skills/<name>/SKILL.md   # type: skill
  commands/*.md            # type: command
  agents/*.md              # type: agent，子代理定义（design/07）
  workflows/*.md           # type: workflow，保存的编排脚本（design/03）
  log.md                   # 可选，变更历史
<monorepo 子目录>/
  AGENTS.md                # 嵌套共识文件（可选）
  .agents/                 # 子项目级，结构同根（按需加载）
```

用户级: `~/.agents/`（个人规则与技能，结构相同）；运行时状态（会话、记忆、缓存）放 `~/.kxen/` 与项目 `.kxen/`，不进 git，与配置知识分离。

## 4. frontmatter 词汇表

```yaml
---
type: rule                  # 必填: rule | reference | skill | command | agent | workflow
title: 构建与测试命令
description: bun 工程的构建、测试、lint 命令与注意事项
priority: high              # high | normal | low，预算裁剪与注入排序用
applyTo: ["packages/**"]    # 路径 glob；命中才注入；缺省看 alwaysApply
alwaysApply: false          # true = 每轮注入；false = 仅索引可见
roles: [execution, review]  # 限定哪些角色注入；缺省全部
tags: [build, bun]
timestamp: 2026-07-20T08:00:00Z
---
```

加载语义矩阵：

| type | alwaysApply | applyTo 命中 | 都不满足 |
| --- | --- | --- | --- |
| rule | 每轮注入 | 工作在该路径时注入 | 只在 index.md 可见，模型按需 read |
| reference | （无此字段） | （无此字段） | 永不自动注入，只能经 index.md / 链接按需读取 |
| skill / command / agent / workflow | 按各自语义（懒加载 / 斜杠调用 / 子代理定义 / 编排脚本） | | |

## 5. 多层目录解析

- 启动时加载: 从仓库根到 cwd 路径上每层 `AGENTS.md`（串联不覆盖，近者在后，同 CC 语义）+ 根 `.agents/` 中 `alwaysApply: true` 的 rule + 根 `index.md`
- 按需加载: 模型读取某子目录文件时，该子目录的嵌套 `AGENTS.md`、嵌套 `.agents/index.md`、以及 `applyTo` 命中该路径的 rule 被注入（JIT，与 Claude Code 嵌套 CLAUDE.md 同型）
- 冲突: 同名字段近者胜（子目录覆盖根）；rule 正文是串联不是覆盖
- 压缩后: `alwaysApply` 与命中 rule 全部重注入（不修 CC 的 path-scope 规则压缩后丢失问题，见 analysis/01 C7）

## 6. index.md 与预算

- `kxen sync`（或 `kxen doctor`）扫描 frontmatter 生成每层 `index.md`：按 type 分组，每条带 description；禁止手写（生成物）
- 模型默认只看到根 `index.md` 而非全部 rules；需要深入时 read 对应子目录 `index.md` 或文档
- prompt composer 对注入项做预算（design/08 K-预算）：超限按 priority 裁剪并报告，裁剪事件进事件流
- references 不进预算：它们只在被 read 时按普通文件读取计费

## 7. 与既有决策的关系

- design/08 K1 更新：canonical 从「AGENTS.md 分层 + `.kxen/`」改为「AGENTS.md 分层 + `.agents/`」；`.kxen/` 只留运行时状态
- design/07 子代理定义路径改为 `.agents/agents/*.md`（项目级）与 `~/.agents/agents/`（用户级）
- analysis/07 P1 composer 的「项目规则」注入层按本文档矩阵实现
- std-agent 生态：其 `rules / skills / commands / references / subagents` 五型与本文档 type 词汇一一对应，作为 producer 零转换

## 8. 反模式

- 不把 references 塞进 rules（知识按需读，注入必烧 context）
- 不手写 index.md（生成物，手写必漂移）
- 不在 `.agents/` 放运行时状态（会话、缓存、token 统计归 `.kxen/`）
- 不给 reference 加 alwaysApply（references 永不自动注入）
