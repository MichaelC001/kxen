//! 常驻工具定义（渐进披露：常驻 ~12，其余经 Tool Search——M5）。

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
            "task_output",
            "Get the accumulated output of a background task.",
            json!({
                "type": "object",
                "properties": { "task_id": { "type": "string" } },
                "required": ["task_id"]
            }),
        ),
        ToolDefinition::function(
            "kill_task",
            "Kill a background task.",
            json!({
                "type": "object",
                "properties": { "task_id": { "type": "string" } },
                "required": ["task_id"]
            }),
        ),
        ToolDefinition::function(
            "list_tasks",
            "List all background tasks (dev servers, long commands) with status, uptime, port, output tail.",
            json!({ "type": "object", "properties": {} }),
        ),
        ToolDefinition::function(
            "dev_server",
            "Start a dev server (vite/cargo watch/next dev) in background and block until ready (output pattern or port reachable). Returns task_id and url. Do NOT sleep-wait yourself.",
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "workdir": { "type": "string" },
                    "ready": {
                        "type": "object",
                        "properties": {
                            "pattern": { "type": "string" },
                            "port": { "type": "integer" },
                            "timeout_ms": { "type": "integer" }
                        }
                    }
                },
                "required": ["command", "workdir"]
            }),
        ),
        ToolDefinition::function(
            "task",
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
            "restart_task",
            "Restart a background task with the same command (keeps task_id stable semantics).",
            json!({
                "type": "object",
                "properties": { "task_id": { "type": "string" } },
                "required": ["task_id"]
            }),
        ),
    ]
}
