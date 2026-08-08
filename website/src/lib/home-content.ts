export const homeTitle = "kxen";
export const homeDescription =
  "面向复杂软件工程任务的本地 Coding Agent Harness，桌面覆盖 macOS、Windows、Linux，也可经浏览器访问。";

export const homeBody = `kxen 是一个面向复杂软件工程任务的 Coding Agent Harness。它让用户在一个本地应用或浏览器中组织工作目录、会话、模型、目标和 Agent 执行过程。

当前版本是开发预览。安装包已在 GitHub Releases 公开，桌面应用内置自动更新。

## 下载

下载链接指向 GitHub Releases 的稳定 asset，始终解析到最新版本。

桌面应用:

- macOS： [Apple Silicon](https://github.com/StringKe/kxen/releases/latest/download/kxen-macos-aarch64.dmg) / [Intel](https://github.com/StringKe/kxen/releases/latest/download/kxen-macos-x86_64.dmg)。DMG 经 Developer ID 签名和 Apple 公证，打开后把 Kxen.app 拖入「应用程序」。需要 macOS 14 及以上版本。
- Windows： [x64](https://github.com/StringKe/kxen/releases/latest/download/kxen-windows-x86_64-setup.exe) / [ARM64](https://github.com/StringKe/kxen/releases/latest/download/kxen-windows-aarch64-setup.exe)。安装包暂未做 Authenticode 签名，SmartScreen 提示时选择 More info -> Run anyway。
- Linux x86_64： [AppImage](https://github.com/StringKe/kxen/releases/latest/download/kxen-linux-x86_64.AppImage) / [deb](https://github.com/StringKe/kxen/releases/latest/download/kxen-linux-x86_64.deb)。
- Linux ARM64： [AppImage](https://github.com/StringKe/kxen/releases/latest/download/kxen-linux-aarch64.AppImage) / [deb](https://github.com/StringKe/kxen/releases/latest/download/kxen-linux-aarch64.deb)。

kxen 无头 server（在服务器或本机运行后用浏览器访问全部功能）:

- macOS： [Apple Silicon](https://github.com/StringKe/kxen/releases/latest/download/kxen-macos-aarch64.tar.gz) / [Intel](https://github.com/StringKe/kxen/releases/latest/download/kxen-macos-x86_64.tar.gz)。
- Linux： [x86_64](https://github.com/StringKe/kxen/releases/latest/download/kxen-linux-x86_64.tar.gz) / [ARM64](https://github.com/StringKe/kxen/releases/latest/download/kxen-linux-aarch64.tar.gz)。
- Windows： [x64](https://github.com/StringKe/kxen/releases/latest/download/kxen-windows-x86_64.zip) / [ARM64](https://github.com/StringKe/kxen/releases/latest/download/kxen-windows-aarch64.zip)。

每个版本附带 SHA256SUMS 和 updater 签名，可用于校验下载产物。全部 asset 与历史版本见 [GitHub Releases](https://github.com/StringKe/kxen/releases/latest)。

## 产品文档

- [产品概览](https://kxen.ai/overview/)
- [开始使用](https://kxen.ai/getting-started/)
- [Web 模式](https://kxen.ai/getting-started/web-mode)
- [Workspace](https://kxen.ai/workspace/workspace/)
- [模型与 Provider](https://kxen.ai/models/)
- [Agent 与任务](https://kxen.ai/agent/)
- [知识与定制](https://kxen.ai/knowledge/)
- [集成能力](https://kxen.ai/integrations/)
- [恢复与隔离](https://kxen.ai/recovery/)
- [参考手册](https://kxen.ai/reference/)
- [核心概念](https://kxen.ai/concepts/)

## 稳定边界

- 桌面平台覆盖 macOS（Apple Silicon 和 Intel）、Windows（x64 和 ARM64）、Linux（x86_64 和 ARM64）;kxen 无头 server 覆盖同样的六个平台。
- 应用形态是 Tauri 2 桌面应用和 kxen 无头 server；桌面 webview 与浏览器是同一内嵌服务的两个平等客户端，经同一个 /ws 端点使用全部功能。除 kxen 无头 server 外，不提供其他 CLI、TUI 或公开 HTTP API。
- Rust 后端拥有运行状态，SolidJS 前端负责交互和呈现。
- 所有模型调用进入统一资源管理层。
- 高风险工具调用在执行层统一审批或拒绝。
- 文件删除进入废纸篓，不直接执行不可恢复删除。`;

export const homeSource = `---
title: ${homeTitle}
description: ${homeDescription}
---

${homeBody}
`;
