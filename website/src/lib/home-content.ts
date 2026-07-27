export const homeTitle = "kxen";
export const homeDescription = "macOS Apple Silicon 原生 Coding Agent Harness 的官网与权威文档。";

export const homeBody = `kxen 是一个面向复杂软件工程任务的原生 Coding Agent Harness。它把模型调用、目标管理、动态工作流、子代理、工具执行、安全边界和本地知识放进同一个 macOS 应用。

当前版本是开发预览。公开发行、签名下载和自动更新尚未开放。

## 产品文档

- [产品概览](https://kxen.ai/overview/)
- [开始使用](https://kxen.ai/getting-started/)
- [使用指南](https://kxen.ai/guides/)
- [参考手册](https://kxen.ai/reference/)
- [核心概念](https://kxen.ai/concepts/)

## 稳定边界

- 平台限定为 macOS 14 及以上版本和 Apple Silicon。
- 应用形态是 Tauri 2 桌面应用，不提供 CLI、TUI 或公开 HTTP API。
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
