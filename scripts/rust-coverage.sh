#!/usr/bin/env bash
set -euo pipefail

KXEN_COVERAGE_IGNORE='(^|/)(main|app_state|os_notify)\.rs$|(^|/)ws/|agent/agent_loop/(execute|run|run_calls)\.rs$|agent/team/member_loop\.rs$|auth/probe/sources\.rs$|knowledge/(consolidate|embedding)\.rs$|llm/(anthropic|client|models|openai|verify|xai)\.rs$|lsp/(mod|process)\.rs$|mcp/(oauth_flow|remote_sse|transport)\.rs$|tools/browser/chrome\.rs$|tools/webfetch\.rs$|tools/websearch/|voice/(apple|mod|objc|provider)\.rs$'

# Tauri host、macOS Objective-C、外部进程和网络 adapters 需要 live acceptance，确定性 line gate 只衡量可隔离 core。
cargo llvm-cov \
  --manifest-path src-tauri/Cargo.toml \
  --all-targets \
  --all-features \
  --summary-only \
  --ignore-filename-regex "$KXEN_COVERAGE_IGNORE" \
  --fail-under-lines 80
