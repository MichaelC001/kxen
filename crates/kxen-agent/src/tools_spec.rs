//! Resident tool definitions (progressive disclosure: ~10 resident, rest via Tool Search in M5).
//! All tool descriptions are English by design; UI strings stay Simplified Chinese.

use kxen_llm::tool::ToolDefinition;
use serde_json::json;

pub fn core_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::function(
            "exec",
            "Execute a command in an explicitly declared shell dialect (zsh/bash/fish). Long commands auto-background after 15s and return a task_id - you are notified on completion, so do not poll or sleep-wait. Prefer one well-formed command over chained one-liners.",
            json!({
                "type": "object",
                "properties": {
                    "type": { "type": "string", "enum": ["zsh", "bash", "fish"], "description": "REQUIRED shell dialect" },
                    "path": { "type": "string", "description": "Working directory" },
                    "command": { "type": "string" },
                    "timeout_ms": { "type": "integer" },
                    "background": { "type": "boolean", "description": "Run in background, returns task_id immediately" }
                },
                "required": ["type", "path", "command"]
            }),
        ),
        ToolDefinition::function(
            "read",
            "Read a file with LINE#HASH anchors for later anchored edits.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        ToolDefinition::function(
            "edit",
            "Edit a file. Prefer anchors mode: read outputs lines as `LINE#HASH  content`, pass that anchor directly in edits[].anchor (e.g. `3#a1b2`). Match mode needs exact old_string. No need to read first if the file was read this session and unchanged.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "mode": { "type": "string", "enum": ["anchors", "match"] },
                    "edits": { "type": "array", "items": { "type": "object", "properties": { "anchor": { "type": "string" }, "new_text": { "type": "string" } }, "required": ["anchor", "new_text"] } },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" },
                    "expected_replacements": { "type": "integer" }
                },
                "required": ["path", "mode"]
            }),
        ),
        ToolDefinition::function(
            "write",
            "Write a file (creates parent dirs; backs up before overwriting an externally-changed file).",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
        ),
        ToolDefinition::function(
            "delete",
            "Delete a file to the Trash (recoverable).",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        ToolDefinition::function(
            "task",
            "Manage background tasks (dev servers, long-running commands). Actions: start (spawn in background; pass `ready` to block until the server is ready - pattern matched in output or port reachable - and get back task_id + url), output (accumulated output), kill, list (status/uptime/port/tail), restart (same command, fresh process). Use start with a ready spec for dev servers instead of exec + sleep.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["start", "output", "kill", "list", "restart"] },
                    "task_id": { "type": "string", "description": "Required for output/kill/restart" },
                    "command": { "type": "string", "description": "Required for start" },
                    "workdir": { "type": "string" },
                    "shell": { "type": "string", "enum": ["zsh", "bash", "fish"] },
                    "ready": {
                        "type": "object",
                        "description": "Optional readiness gate for start",
                        "properties": {
                            "pattern": { "type": "string" },
                            "port": { "type": "integer" },
                            "timeout_ms": { "type": "integer" }
                        }
                    }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "goal",
            "Manage durable goals: create with a completion contract (objective + completionCriteria + optional constraints/budget), then drive the lifecycle (activate/pause/resume/complete/cancel/list/get). Goals persist across turns with budgets; same block reason 3 turns in a row escalates to blocked.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "get", "activate", "pause", "resume", "complete", "cancel", "list"] },
                    "id": { "type": "string" },
                    "objective": { "type": "string" },
                    "completion_criteria": { "type": "string" },
                    "constraints": { "type": "string" },
                    "budget": { "type": "object", "properties": { "tokens": { "type": "integer" }, "turns": { "type": "integer" }, "wall_clock_ms": { "type": "integer" } } },
                    "evidence": { "type": "string" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "agent",
            "Dispatch a subagent by role: thinking (deep analysis), planning (task decomposition), execution (fast execution), review (adversarial review), research (external research). Each runs on a model chosen for the role.",
            json!({
                "type": "object",
                "properties": {
                    "role": { "type": "string", "enum": ["thinking", "planning", "execution", "review", "research"] },
                    "prompt": { "type": "string", "description": "The task for the subagent to perform" }
                },
                "required": ["role", "prompt"]
            }),
        ),
        ToolDefinition::function(
            "workflow",
            "Run a JavaScript orchestration script (QuickJS, sandboxed). Globals: `await agent(role, prompt)` -> string (subagent dispatch, MRM-routed); `CONSTRAINTS` (role bindings + provider availability); `phase(name)` (progress marker); `log(msg)`. Use plain JS for control flow: Promise.all for fan-out, for-loops for pipelines. The script return value is the workflow result. Cap: 32 agent dispatches, 10min wall clock.",
            json!({
                "type": "object",
                "properties": {
                    "script": { "type": "string", "description": "JavaScript body wrapped in an async function; use return for the result" }
                },
                "required": ["script"]
            }),
        ),
    ]
}
