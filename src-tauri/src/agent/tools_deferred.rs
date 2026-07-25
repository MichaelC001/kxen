//! deferred 工具目录：默认不进上下文，经 tool_search 挂载到会话。
//! 独立文件是因为 tools_spec.rs 贴近 350 行门禁；描述英文是既定口径（UI 文案才用中文）。

use crate::llm::tool::ToolDefinition;
use serde_json::json;

pub fn deferred_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::function(
            "todo",
            "Session todo list for tracking multi-step work: add items, list, complete by id, clear completed.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["add", "list", "complete", "clear"] },
                    "content": { "type": "string", "description": "Required for add" },
                    "id": { "type": "integer", "description": "Required for complete" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "webfetch",
            "Fetch a URL and return the page as plain text (scripts/styles stripped, capped at 50k chars).",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "https:// or http:// URL" }
                },
                "required": ["url"]
            }),
        ),
        ToolDefinition::function(
            "websearch",
            "Search the web (DuckDuckGo) and return top results with title, URL and snippet. Use for current events, docs, library facts.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "search query" }
                },
                "required": ["query"]
            }),
        ),
        ToolDefinition::function(
            "browser",
            "Drive the system Chrome (headless) over CDP: open/navigate to a URL, snapshot the page as a compact accessibility tree with numbered refs, then click/fill by ref, evaluate JS, screenshot to a file, go back, close. One lazy per-session instance; refs go stale after any navigation or click - snapshot again. Prefer webfetch for read-only text extraction.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["open", "navigate", "snapshot", "click", "fill", "evaluate", "screenshot", "back", "close"] },
                    "url": { "type": "string", "description": "Required for open/navigate: https:// or http:// URL (SSRF-guarded like webfetch)" },
                    "ref": { "type": "integer", "description": "Required for click/fill: element number from the latest snapshot" },
                    "text": { "type": "string", "description": "Required for fill: text to type into the element" },
                    "expr": { "type": "string", "description": "Required for evaluate: JS expression, result returned as JSON (capped at 10KB)" }
                },
                "required": ["action"]
            }),
        ),
    ]
}
