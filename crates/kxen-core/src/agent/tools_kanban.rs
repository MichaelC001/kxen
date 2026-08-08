//! kanban_* 工具目录（P2b 工具面）：deferred 挂载，经 tool_search 发现后挂载到会话。
//! 独立文件守 350 行门禁（tools_deferred.rs 无空位）；描述英文是既定口径（UI 文案才用中文）。
//! 全部工具只提交 KanbanCommand（意图），kanban core 校验通过才转 Event：模型不直写状态
//! （对齐 goal 工具的意图校验模式，design.md「工具面」）。
//! PolicySet/AutoApproved 刻意不在此目录：授权是 human-only 命令，
//! 模型能给自己扩权 = 安全漏洞（P3 看板级自主授权只经核心 API 设置）。

use crate::llm::tool::ToolDefinition;
use serde_json::json;

/// 列定义 schema（board_create.columns / column_add.column 共用）：与 model.rs ColumnDef 同形，
/// serde deny_unknown_fields 在执行侧兜底，schema 只起提示作用。
fn column_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Column id ([A-Za-z0-9_-])" },
            "title": { "type": "string" },
            "on_enter": {
                "type": "object",
                "description": "Entry action. agent_run/workflow require `agent` (a definition in .kxen/kanban/agents/); none/human_gate must not set it",
                "properties": {
                    "kind": { "type": "string", "enum": ["none", "agent_run", "workflow", "human_gate"] },
                    "agent": { "type": "string" }
                },
                "required": ["kind"]
            },
            "transitions": {
                "type": "object",
                "description": "Targets must be existing column ids; a column without an outgoing edge is terminal",
                "properties": {
                    "on_success": { "type": "string" },
                    "on_failure": { "type": "string" }
                }
            },
            "wip_limit": { "type": "integer", "description": "Optional max cards in this column (>= 1)" },
            "timeout_ms": { "type": "integer", "description": "Optional per-column run timeout (>= 1); default 30min" }
        },
        "required": ["id", "title"]
    })
}

pub fn kanban_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::function(
            "kanban_board_create",
            "Create a kanban board in this workspace (.kxen/kanban/). Omit `columns` for the default software-development template: requirements(human_gate) -> implementing(agent_run) -> testing(agent_run) -> review(human_gate) -> done(terminal). Returns the board id used by all other kanban_* tools.",
            json!({
                "type": "object",
                "properties": {
                    "board": { "type": "string", "description": "Optional explicit board id ([A-Za-z0-9_-]); auto-generated when omitted" },
                    "title": { "type": "string" },
                    "columns": { "type": "array", "items": column_schema(), "description": "Optional custom columns; validated as a set (unique ids, transition targets must exist)" }
                },
                "required": ["title"]
            }),
        ),
        ToolDefinition::function(
            "kanban_column_add",
            "Append a column to an existing board. Transition targets must reference columns that already exist on the board (add columns in dependency order).",
            json!({
                "type": "object",
                "properties": {
                    "board": { "type": "string" },
                    "column": column_schema()
                },
                "required": ["board", "column"]
            }),
        ),
        ToolDefinition::function(
            "kanban_card_create",
            "Create a card. Without `column_id` the card lands in the board's first column. WIP limits are enforced; cards in an agent_run/workflow column are picked up by the kanban runner automatically.",
            json!({
                "type": "object",
                "properties": {
                    "board": { "type": "string" },
                    "title": { "type": "string" },
                    "body": { "type": "string", "description": "Card detail, shown to the executing agent as context" },
                    "column_id": { "type": "string", "description": "Target column; defaults to the first column" }
                },
                "required": ["board", "title"]
            }),
        ),
        ToolDefinition::function(
            "kanban_card_move",
            "Move a card by declaring the outcome of its CURRENT column: success = approve/advance, failure = reject/send back. The target column is derived from the column's transitions table - you never name it. human_gate approval is a card_move with outcome=success. Rejected when no transition exists for the outcome or the destination WIP is full.",
            json!({
                "type": "object",
                "properties": {
                    "board": { "type": "string" },
                    "card_id": { "type": "string" },
                    "outcome": { "type": "string", "enum": ["success", "failure"] }
                },
                "required": ["board", "card_id", "outcome"]
            }),
        ),
        ToolDefinition::function(
            "kanban_card_comment",
            "Append a comment to a card's event stream (visible to future column runs as context). The card stays in place; comments never move cards.",
            json!({
                "type": "object",
                "properties": {
                    "board": { "type": "string" },
                    "card_id": { "type": "string" },
                    "body": { "type": "string" },
                    "author": { "type": "string", "description": "Optional comment author label; defaults to 'agent'" }
                },
                "required": ["board", "card_id", "body"]
            }),
        ),
        ToolDefinition::function(
            "kanban_agent_create",
            "Define a DCP agent for kanban column runs: validates the definition, writes .kxen/kanban/agents/<name>.md and registers it on the board. permission_profile decides the agent's tool set: readonly (read/glob/grep), readonly+test (+ exec), full (all resident tools), custom (exactly the tools you list). custom REQUIRES `tools` (closed allowlist: read, glob, grep, edit, write, delete, exec, lsp, webfetch, websearch, todo, task, goal, knowledge, skill) and the fixed profiles must NOT set it - e.g. a Go editing agent: permission_profile='custom', tools=['read','glob','grep','edit','write','exec','lsp']. model is 'auto' (MRM-routed by role) or 'provider:model'.",
            json!({
                "type": "object",
                "properties": {
                    "board": { "type": "string" },
                    "name": { "type": "string", "description": "Agent id ([A-Za-z0-9_-]); column on_enter.agent references this" },
                    "role": { "type": "string", "description": "MRM routing role used when model = auto" },
                    "model": { "type": "string", "description": "'auto' or 'provider:model'" },
                    "permission_profile": { "type": "string", "enum": ["readonly", "readonly+test", "full", "custom"] },
                    "tools": { "type": "array", "items": { "type": "string" }, "description": "REQUIRED for permission_profile='custom' (exact tool set, closed allowlist); forbidden for the fixed profiles" },
                    "prompt": { "type": "string", "description": "Agent brief (system prompt body); for workflow columns this is the QuickJS script instead" }
                },
                "required": ["board", "name", "role", "model", "permission_profile", "prompt"]
            }),
        ),
        ToolDefinition::function(
            "kanban_agent_run",
            "Explicitly run the current column's agent on a card: durably claims a run (run_started event); the kanban runner adopts and executes the claim asynchronously - poll kanban_board_show for the outcome. Use for starting ready cards and retrying blocked/timed-out ones. Rejected while a run is in progress or when the column has no agent_run/workflow entry action.",
            json!({
                "type": "object",
                "properties": {
                    "board": { "type": "string" },
                    "card_id": { "type": "string" }
                },
                "required": ["board", "card_id"]
            }),
        ),
        ToolDefinition::function(
            "kanban_board_show",
            "Show the current board state: columns (entry action, WIP), cards (status, current run, block reason), runs with outcomes, and defined agents. Read-only; take board/card ids from this or from create responses, never invent them.",
            json!({
                "type": "object",
                "properties": {
                    "board": { "type": "string" }
                },
                "required": ["board"]
            }),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_cover_the_full_p2b_surface() {
        let names: Vec<_> = kanban_tools().into_iter().map(|tool| tool.function.name).collect();
        assert_eq!(
            names,
            [
                "kanban_board_create",
                "kanban_column_add",
                "kanban_card_create",
                "kanban_card_move",
                "kanban_card_comment",
                "kanban_agent_create",
                "kanban_agent_run",
                "kanban_board_show",
            ]
        );
    }

    #[test]
    fn policy_commands_are_never_exposed_as_tools() {
        // 防回归：PolicySet/AutoApproved 是 human-only 命令，模型工具目录永不得出现授权面
        let names: Vec<_> = kanban_tools().into_iter().map(|tool| tool.function.name).collect();
        assert!(!names.iter().any(|name| name.contains("policy") || name.contains("approve")), "授权命令不得进模型工具目录: {names:?}");
    }
}
