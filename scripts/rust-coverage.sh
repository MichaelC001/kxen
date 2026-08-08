#!/usr/bin/env bash
set -euo pipefail

KXEN_COVERAGE_IGNORE='(^|/)(main|app_state|os_notify)\.rs$|(^|/)ws/(llm_task|ops|ops_agents|ops_attach|ops_diagnostics|ops_knowledge|ops_mcp|ops_provider|ops_workspace|pending|rpc|settings|worktree_rpc)\.rs$|(^|/)ws/llm_task/spawn\.rs$|(^|/)ws/rpc/workspace_activation\.rs$|agent/agent_loop/(execute|run|run_calls)\.rs$|agent/team/member_loop\.rs$|auth/probe/sources\.rs$|auth/refresh\.rs$|(^|/)auth/oauth_login/(code_flow|device_flow|mod)\.rs$|(^|/)auth/oauth_login/device_flow/aws_sso\.rs$|llm/(anthropic|client|kiro|models|openai|verify|xai)\.rs$|lsp/(mod|process)\.rs$|mcp/(oauth_flow|remote_sse|transport)\.rs$|tools/browser/chrome\.rs$|tools/webfetch\.rs$|tools/websearch/|voice/(apple|objc|provider)\.rs$'

# 仅忽略 Tauri host dispatch、macOS Objective-C、外部进程和真实网络 adapters。
# 拆分后的 host adapter 只按精确文件名忽略；provider 并发写回、Session 删除和恢复、
# active context、queue/terminal lifecycle、voice lifecycle 等确定性核心逻辑必须计入 line gate。
# oauth_login 的 code_flow/device_flow/mod 是浏览器授权 + loopback 回调 + 设备轮询的真实网络适配器
# （与 mcp/oauth_flow.rs 同类）；auth/refresh.rs 与 llm/kiro.rs 是 token 端点/推理端点 HTTP 适配器
# （与 llm/anthropic.rs 同类）。纯逻辑（spec/zai_zcode/aws_sso/grant/wire/eventstream/stream）必须计入。
cargo llvm-cov \
  \
  --package kxen-core \
  --all-targets \
  --all-features \
  --summary-only \
  --ignore-filename-regex "$KXEN_COVERAGE_IGNORE" \
  --fail-under-lines 80
