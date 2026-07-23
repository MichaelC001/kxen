# 插件系统设计（G3）

- 状态：设计稿待确认，未开始实现
- 依据：docs/analysis/kxen-competitive-analysis-2026-07-23.md（生态层：无插件系统、无市场）

## 目标

在**既有统一知识树**之上做插件分发与生命周期管理：插件 = 一个目录包（skills/ commands/ rules/ hooks/ mcp servers 的组合），可安装/启用/禁用/删除。**不发明新的插件格式**——复用 `.agents/` 树与 frontmatter 超集。

## 核心判断

kxen 的知识系统（OKF 单规范）已经把 rules/skills/commands 统一成 Entry；插件层缺的只是「打包、来源、启用状态、安装入口」，而不是第二种扩展机制。反对做 marketplace 服务型分发（运维成本超出当前阶段），v1 来源只支持本地目录 + git URL。

## 插件形态

```
my-plugin/
├── plugin.toml       # 清单：name/version/description + 启用面
├── skills/           # 并入 <scope>/.agents/skills/
├── commands/         # 并入 <scope>/.agents/commands/
├── rules/            # 并入 <scope>/.agents/rules/
├── hooks.toml        # 并入 config [hooks]（信任门管控）
└── mcp.json          # 并入 MCP server 配置（依赖 G1）
```

- 安装 = 复制到 `~/.agents/plugins/<name>/` 并在 `plugins.toml` 登记 enabled
- 生效 = knowledge scan 增加一个扫描根：`~/.agents/plugins/<enabled>/`（其内子目录按既有 kind 推断，与现有树完全同构）
- hooks/mcp 内容只在项目已信任或用户显式批准时生效（复用 core/trust.rs）

## 架构

```
src-tauri/src/plugins/
├── mod.rs        // PluginManager：install/enable/disable/remove/list
├── manifest.rs   // plugin.toml 解析（name/version/description）
└── registry.rs   // plugins.toml 登记（enabled 状态持久化）
```

- `knowledge/scan.rs` 的 roots 追加 `~/.agents/plugins/*/enabled=true` 目录（零格式改动收益）
- RPC：`plugin.list / plugin.install(path|git_url) / plugin.enable / plugin.disable / plugin.remove`
- 设置页「高级」区：插件列表（启停开关 + 来源 + 删除）

## 非目标（v1）

- 在线市场/搜索/评分、签名与审计、版本升级与依赖解析、JS/WASM 插件运行时

## 里程碑

1. manifest/registry + install（本地目录复制）
2. scan roots 并入 + 启停热生效
3. git URL 安装（git clone 浅拉）
4. 设置页管理面板
5. hooks.toml/mcp.json 并入（依赖 G1 完成后）

## 验证

- 单测：manifest 解析、registry 启停持久化、安装后 scan 命中插件 skill
- 集成：造一个含 1 skill + 1 command 的示例插件，安装后 / 弹窗与 skill_load 均可见
