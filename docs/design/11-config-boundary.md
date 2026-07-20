# 配置边界：pi SettingsManager vs kxen config.toml

- 日期: 2026-07-20
- 结论: 两套并存，各管各的域，不重复

## 边界

| 域 | 归属 | 机制 |
| --- | --- | --- |
| pi 域设置（model / thinking / theme / compaction / retry / terminal / images / keybindings 等） | pi SettingsManager（settings.json，全局 + 项目分层） | 通过 pi runtime 内置使用，kxen 不碰 |
| kxen 域设置（roles / limits / budgets / MRM 相关） | `@kxen/core` config.toml（TOML，Bun.TOML.parse） | pi 没有这些概念，kxen 自有 |

判断规则（以后新增配置项时执行）：

- 配置项语义属于「模型 / 会话 / TUI / 压缩」-> 进 pi settings，不加进 config.toml
- 配置项语义属于「角色路由 / 资源调度 / 预算」-> 进 config.toml
- 两边都不该有时（一次性开关、调试项）-> 环境变量（见 design/10），不进任何配置文件

## API 使用约定（Bun 原生优先）

- 文件读写: `Bun.file()` / `Bun.write()`（自动创建父目录，不需要 mkdir）
- 目录扫描: `Bun.Glob`（替代 readdirSync 递归）
- 配置解析: `Bun.TOML.parse`
- 进程: `Bun.spawn`
- 仅在无 Bun 等价物时用 node:：`chmod` / `rename` / `createWriteStream`（流式编码）/ `child_process`（vscode-jsonrpc 需要 node 流）/ `tmpdir` / 测试辅助（mkdtemp 等）
