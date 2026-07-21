# kxen

macOS Apple Silicon 原生 Coding Agent Harness（Tauri 2 + Rust + QuickJS）。

## 特性

- 四订阅混用：Claude / Codex / Grok Build / Kimi Code（OAuth 复用，非 API key）
- 角色化 subagent：thinking / planning / execution / review / research，经 MRM 全局资源调度
- goal 生命周期：create -> activate -> 执行 -> 验证 -> complete/blocked，带预算与阻塞升级
- workflow：模型自编 JavaScript 编排脚本（QuickJS 沙箱），agent()/CONSTRAINTS/phase() 原语
- 命令调度：快照 shell + auto_bg（15s 自动后台）+ dev_server 就绪门 + rm 强制进回收站
- loop 检测四层：exact / semantic / stagnation / churn，空转硬停

## 开发

```bash
cargo test          # 全部测试（47 个）
cargo run           # 启动 app（dev）
cd ui && vp dev     # 前端 dev server（端口 7823）
```

设计文档：`docs/rust/01-design.md`
