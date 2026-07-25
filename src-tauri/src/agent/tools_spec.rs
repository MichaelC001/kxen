//! Resident tool definitions (progressive disclosure: ~10 resident, rest via Tool Search in M5).
//! All tool descriptions are English by design; UI strings stay Simplified Chinese.

use crate::llm::tool::ToolDefinition;
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
            "Read a file with LINE#HASH anchors for later anchored edits. Returns at most 2000 lines per call; for larger files page with offset (1-based) and limit - the output notes the shown range and total line count.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "offset": { "type": "integer", "description": "1-based first line to return, defaults to 1" },
                    "limit": { "type": "integer", "description": "Max lines to return, defaults to 2000 (hard cap)" }
                },
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
            "Manage durable goals with a completion contract. Actions: create (requires BOTH objective and completion_criteria strings; constraints/budget optional; the response contains the new goal id), activate/pause/resume/cancel/get (require id - always take it from a create or list response, never invent one), complete (requires id AND concrete verification evidence, min 20 chars, not a placeholder like 'done'), list (no params). Goals persist across turns; same block reason 3 turns in a row escalates to blocked.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "get", "activate", "pause", "resume", "complete", "cancel", "list"] },
                    "id": { "type": "string", "description": "Goal id from create/list response" },
                    "objective": { "type": "string", "description": "REQUIRED for create: what must become true" },
                    "completion_criteria": { "type": "string", "description": "REQUIRED for create: the observable proof of done, e.g. 'head -1 README.md prints # kxen'" },
                    "constraints": { "type": "string" },
                    "budget": { "type": "object", "properties": { "tokens": { "type": "integer" }, "turns": { "type": "integer" }, "wall_clock_ms": { "type": "integer" } } },
                    "evidence": { "type": "string" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "glob",
            "Find files by glob pattern (respects .gitignore), sorted by recency. Examples: `**/*.rs`, `src/**/*.toml`.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string", "description": "Base directory, defaults to working directory" }
                },
                "required": ["pattern"]
            }),
        ),
        ToolDefinition::function(
            "grep",
            "Search file contents with a regex (respects .gitignore). Returns `path:line: content` matches.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern" },
                    "path": { "type": "string", "description": "Base directory, defaults to working directory" },
                    "glob": { "type": "string", "description": "Optional file filter, e.g. `*.rs`" }
                },
                "required": ["pattern"]
            }),
        ),
        ToolDefinition::function(
            "lsp",
            "Language-server intelligence for rust, ts/tsx, js/jsx, python and go files (per-language servers start lazily on first use; a language whose server is not installed degrades to a hint message while other languages keep working). Actions: diagnostics (default; pass `path` for one file, omit for all session-touched supported files), hover/definition/references (require `path`, `line`, `character`, 1-based), symbols (document outline, requires `path`).",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["diagnostics", "hover", "definition", "references", "symbols"], "description": "Defaults to diagnostics" },
                    "path": { "type": "string", "description": "File path (relative to working directory); required for hover/definition/references/symbols" },
                    "line": { "type": "integer", "description": "1-based line, required for hover/definition/references" },
                    "character": { "type": "integer", "description": "1-based column, required for hover/definition/references" }
                }
            }),
        ),
        ToolDefinition::function(
            "tool_search",
            "Discover additional tools that are not loaded by default (progressive disclosure). Returns matching tool cards; matched tools become callable for the rest of this session.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What you need, e.g. 'todo list' or 'fetch url'" }
                },
                "required": ["query"]
            }),
        ),
        ToolDefinition::function(
            "agent",
            "Dispatch a subagent by role: thinking (deep analysis), planning (task decomposition), execution (fast execution), review (adversarial review), research (external research). Each runs on a model chosen for the role. Default is synchronous (blocks until the subagent finishes); set background=true for 2+ independent tasks to run them in parallel - the call returns a receipt immediately and each result arrives later as a task notification.",
            json!({
                "type": "object",
                "properties": {
                    "role": { "type": "string", "enum": ["thinking", "planning", "execution", "review", "research"] },
                    "prompt": { "type": "string", "description": "The task for the subagent to perform" },
                    "worktree": { "type": "string", "description": "Optional: run this dispatch inside an isolated git worktree with this name (branch kxen/<name>, main tree untouched)" },
                    "background": { "type": "boolean", "description": "Optional, default false. true = async dispatch: receipt now, result delivered as a task notification in a later turn" }
                },
                "required": ["role", "prompt"]
            }),
        ),
        ToolDefinition::function(
            "worktree",
            "Manage isolated git worktrees under .kxen/worktrees (for parallel or bulk-change isolation). Actions: create (name), remove (name, delete_branch?), list, diff (name -> diff --stat vs main tree).",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "remove", "list", "diff"] },
                    "name": { "type": "string" },
                    "delete_branch": { "type": "boolean" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "workflow",
            "Run a JavaScript orchestration script (QuickJS, sandboxed) that fans work out to subagents in parallel. MANDATORY for /ultracode, /ultraplan, /ultrareview and any task needing 2+ subagents or named phases - never explore repos one-by-one when a workflow applies. Globals: `await agent(role, prompt)` or `agent(prompt, { agentType, label })` -> string (subagent dispatch, MRM-routed); `await parallel(thunks, { concurrency: 8 })` -> array in input order, failed items come back as `{ __failed: true, error }` instead of rejecting (check and retry/report them); `CONSTRAINTS` (role bindings + provider availability); `phase(name)` (progress marker); `log(msg)`. Optional `export const meta = { name, description, whenToUse, phases: [{ title, detail }] }` enables structured phase progress (index/total per phase call). The script return value is the workflow result; the engine appends a compact completion envelope (agent counts, failures list, phase progress, wall time). Cap: 32 agent dispatches, 10min wall clock. Optional run_id enables resume: re-run with the same run_id and completed agent dispatches return cached results instead of re-dispatching (crash/cancel recovery).",
            json!({
                "type": "object",
                "properties": {
                    "script": { "type": "string", "description": "Flat top-level JavaScript statements (auto-wrapped in async - do NOT wrap in a function yourself); must end with a top-level return of ONE concatenated markdown string" },
                    "run_id": { "type": "string", "description": "optional: stable id to enable journal/resume across runs" }
                },
                "required": ["script"]
            }),
        ),
        ToolDefinition::function(
            "skill",
            "Load a skill by name (see Available skills). Skills are reusable instruction packs; loading one already loaded with identical args is rejected. Do not call for skills marked disable-model-invocation.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["load"] },
                    "name": { "type": "string" },
                    "args": { "type": "string", "description": "Arguments passed to the skill template" }
                },
                "required": ["action", "name"]
            }),
        ),
        ToolDefinition::function(
            "team",
            "Lead an agent team. spawn (name, role, prompt, model? as provider/model, plan_approval?) creates a teammate with its own context and model; message (name, text) sends to its inbox; approve/reject (name, feedback?) answers a plan approval request; shutdown (name); task_create (title, depends_on?); task_cancel (id) cancels a non-completed task; task_fail (id, reason?) marks a non-terminal task as failed and cascades to pending dependents; task_reassign (id, to?) returns a task to the pool and optionally notifies a new owner; list shows members and tasks. Teammates report back automatically - do not poll. Example: {\"action\":\"spawn\",\"name\":\"a\",\"role\":\"execution\",\"model\":\"anthropic/claude-sonnet-4-5-20250929\",\"prompt\":\"task brief\"}.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["spawn", "message", "approve", "reject", "shutdown", "list", "task_create", "task_cancel", "task_fail", "task_reassign"] },
                    "name": { "type": "string" },
                    "role": { "type": "string", "enum": ["thinking", "planning", "execution", "review", "research", "observer"], "description": "observer = receives copies of all team traffic" },
                    "prompt": { "type": "string", "description": "REQUIRED for spawn: the teammate's standing task brief (never 'text')" },
                    "model": { "type": "string", "description": "provider/model override, e.g. anthropic/claude-sonnet-4-5-20250929" },
                    "plan_approval": { "type": "boolean" },
                    "text": { "type": "string", "description": "REQUIRED for message: the message body to deliver" },
                    "feedback": { "type": "string" },
                    "title": { "type": "string" },
                    "depends_on": { "type": "array", "items": { "type": "integer" } },
                    "id": { "type": "integer", "description": "task id for task_cancel/task_fail/task_reassign" },
                    "reason": { "type": "string", "description": "why the task failed (for task_fail)" },
                    "to": { "type": "string", "description": "optional teammate to notify on task_reassign" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "send_message",
            "(teammate only) Send a message to the lead or another teammate by name.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["send"] },
                    "to": { "type": "string" },
                    "text": { "type": "string" }
                },
                "required": ["action", "to", "text"]
            }),
        ),
        ToolDefinition::function(
            "team_task",
            "(teammate only) Shared team task list: claim (next unblocked unassigned), complete (id), fail (id, reason?) marks your own in-progress task as failed, list.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["claim", "complete", "fail", "list"] },
                    "id": { "type": "integer" },
                    "reason": { "type": "string", "description": "why the task failed (for fail)" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "knowledge",
            "Persist durable learnings. add (scope: project|personal, type: correction|convention|pitfall|preference, description, content, slug?) writes one atomic note - same slug replaces, never duplicates. project = true only about this codebase (use sparingly; committed at .agents/notes); personal = cross-project (~/.agents/notes, the default). list shows both scopes; remove (scope, slug).",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["add", "list", "remove"] },
                    "scope": { "type": "string", "enum": ["project", "personal"] },
                    "slug": { "type": "string" },
                    "type": { "type": "string", "enum": ["correction", "convention", "pitfall", "preference", "note"] },
                    "description": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "schedule",
            "Cron-based scheduled agent wakeups (in-process, lives with the app). add (cron 5-field, prompt, once?) schedules a run in THIS session at each fire time; list shows jobs with next fire; remove (id). Use for reminders, periodic checks, or one-shot follow-ups.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["add", "list", "remove"] },
                    "cron": { "type": "string", "description": "5-field cron, e.g. '30 9 * * *' or '*/10 * * * *'" },
                    "prompt": { "type": "string" },
                    "once": { "type": "boolean" },
                    "id": { "type": "string" }
                },
                "required": ["action"]
            }),
        ),
    ]
}

/// deferred 工具目录：默认不进上下文，经 tool_search 挂载到会话。
/// 实现移至 tools_deferred.rs（本文件贴近 350 行门禁）；转口保持既有调用路径不变。
pub use crate::agent::tools_deferred::deferred_tools;
