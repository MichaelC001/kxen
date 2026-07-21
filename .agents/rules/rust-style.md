---
type: rule
alwaysApply: true
description: Rust 代码纪律（性能与安全优先）
---

# Rust 代码纪律

- 少 Clone：共享字符串用 `crate::core::shared::SharedStr`（Arc<str>）；路径用 Arc<Path>；事件回调用 Arc<dyn Fn + Send + Sync>
- 禁 unsafe；lock 一律 `crate::core::shared::lock()`（poison 取回）
- 禁乱 unwrap/expect：库代码错误走 thiserror；lock 中毒经 shared::lock 恢复
- 注释只写 WHY，简体中文；给 AI 的提示词（工具描述/system prompt/role brief）用英文
