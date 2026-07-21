# Security

## 报告安全问题

请通过 GitHub Security Advisory 报告：https://github.com/StringKe/kxen/security/advisories

不接受 AI 生成的批量安全报告。

## Threat Model

### 概述

kxen 是运行在本机的 AI coding harness，agent 拥有 shell 执行、文件读写、网络访问等能力。

### 无沙箱

kxen **不做沙箱隔离**。权限系统（permission prompts）是 UX 层确认机制，不是安全边界。需要真隔离请在容器或 VM 内运行。

### 灾难操作硬防护

与权限系统不同，kxen-safety 的规则族（F1-F5：毁系统 / 毁用户目录 / 删 git 仓库 / 数据与基础设施毁灭 / 批量失控）在执行层硬拦截，**不可被提示词、AGENTS.md、项目规则或权限配置覆盖**。命中时返回结构化错误（规则 id + 原因 + 替代建议）。规则集见 `docs/design/05-safety-rules.md`。

### 凭证

订阅凭证（OAuth token）存储在 `~/.local/share/kxen/auth.json`（0600 权限），由 kxen-auth 从官方 CLI 凭证存储导入（macOS Keychain / `~/.codex` / `~/.grok` / `~/.kimi-code`）。kxen 不上传凭证到任何第三方；daemon 默认仅监听 127.0.0.1。
