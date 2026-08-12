use crate::llm::tool::ToolDefinition;
use serde_json::json;

pub(super) fn all() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::function(
            "bot_message",
            "Send an asynchronous request or response to an explicit Bot peer, or append a timeline-only notice/artifact. Sender identity, Run lineage, depth and hop count are injected by the runtime.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["send_request", "send_response", "post_notice", "post_artifact"] },
                    "target_bot_id": { "type": "string" },
                    "text": { "type": "string" },
                    "task_id": { "type": "string" },
                    "artifact_id": { "type": "string" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "bot_task",
            "Create or update a durable CollaborationTask. Create always requires one target Bot and returns immediately; it never waits synchronously for the peer Agent loop.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "start", "need_input", "need_approval", "complete", "fail", "reject", "cancel"] },
                    "target_bot_id": { "type": "string" },
                    "task_id": { "type": "string" },
                    "title": { "type": "string" },
                    "input": { "type": "string" },
                    "expected_output": { "type": "string" },
                    "result": { "type": "string" },
                    "prompt": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "bot_memory",
            "Read or propose a versioned change to this Bot's durable Memory. Secrets and credentials are always rejected.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "propose_create", "propose_revise", "propose_remove"] },
                    "item_id": { "type": "string" },
                    "kind": { "type": "string", "enum": ["fact", "preference", "procedure", "constraint"] },
                    "content": { "type": "string" },
                    "expected_item_version": { "type": "integer" }
                },
                "required": ["action"]
            }),
        ),
        ToolDefinition::function(
            "bot_artifact",
            "Commit immutable text content to the application Artifact store. The runtime injects Bot ownership; sharing is limited to the current Conversation and must be explicit.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["commit"] },
                    "display_name": { "type": "string" },
                    "media_type": { "type": "string" },
                    "content": { "type": "string" },
                    "share_with_conversation": { "type": "boolean" }
                },
                "required": ["action", "display_name", "media_type", "content"]
            }),
        ),
    ]
}
