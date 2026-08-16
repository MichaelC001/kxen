//! 动态工具生命周期工具定义（tool_define 注册 / tool_undefine 卸载）。
//! 独立文件守 350 行门禁（tools_spec.rs 承载常驻 catalog 主体）；语义见 agent/dynamic.rs。

use crate::llm::tool::ToolDefinition;
use serde_json::json;

/// 常驻 catalog 内的动态工具生命周期定义：紧随 tool_search 之后、workflow 之前。
pub(crate) fn lifecycle_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::function(
            "tool_define",
            "Define a dynamic tool: a QuickJS-sandboxed JavaScript implementation that may compose existing permitted tools via `await tool(name, args)`. Registration requires user approval (full source is shown in the approval card). The tool becomes callable as dyn__<name>_<hash8> where hash8 derives from the implementation - redefining with changed source yields a NEW name, never an overwrite. The script sees a deep-frozen `args` object and must end with a top-level return of one string. Dynamic tools cannot call tool_define, workflow, or other dyn__ tools. Session-scoped: survives fork/resume via the session event stream.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "ASCII [A-Za-z0-9_-] tool name segment" },
                    "description": { "type": "string", "description": "one-line purpose shown to future turns" },
                    "parameters": { "type": "object", "description": "JSON Schema object for the tool arguments (defaults to an empty object schema)" },
                    "implementation": { "type": "string", "description": "JavaScript source: flat top-level statements; `args` holds the validated arguments; `await tool(name, args)` composes existing tools; a top-level return yields the result string" }
                },
                "required": ["name", "description", "implementation"]
            }),
        ),
        ToolDefinition::function(
            "tool_undefine",
            "Remove a dynamic tool previously defined in this session, by its qualified name dyn__<name>_<hash8>. Removal requires user approval and is persisted to the session event stream (consistent across fork/resume); past dyn__ calls in history are unchanged, new calls to the removed name fail closed. Not available in DCP proposal mode - manage the macro directory instead.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "qualified dynamic tool name, e.g. dyn__greet_1a2b3c4d" }
                },
                "required": ["name"]
            }),
        ),
    ]
}
