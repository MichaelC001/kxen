use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::core::identity::{ContentHash, ResourceId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextLayer {
    Platform,
    Definition,
    Execution,
    Memory,
    Conversation,
    CollaborationTask,
    RunHistory,
    NewInput,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VisibilityRef {
    Owner,
    Bot { bot_id: ResourceId },
    Conversation { conversation_id: ResourceId, visible_from_seq: u64 },
    Run { run_id: ResourceId },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderNeutralPart {
    Text { text: String },
    Data { schema_id: ResourceId, fields: BTreeMap<String, String> },
    ToolCall { call_id: ResourceId, tool_name: ResourceId, arguments_json: String },
    ToolResult { call_id: ResourceId, content: String, is_error: bool },
    Artifact { artifact_id: ResourceId, content_hash: ContentHash, media_type: String, display_name: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSegment {
    pub stable_id: ResourceId,
    pub layer: ContextLayer,
    pub order_key: String,
    pub visibility: VisibilityRef,
    pub parts: Vec<ProviderNeutralPart>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextFrame {
    pub source_version: ContentHash,
    pub segments: Vec<ContextSegment>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextCursor {
    pub after_conversation_seq: u64,
    pub after_run_seq: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TurnCursor(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRecordKind {
    Request,
    Response,
    ToolIntent,
    ToolResult,
    Approval,
    Input,
    Checkpoint,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TurnRecord {
    pub record_id: ResourceId,
    pub kind: TurnRecordKind,
    pub parts: Vec<ProviderNeutralPart>,
    pub created_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnReceipt {
    pub cursor: TurnCursor,
}
