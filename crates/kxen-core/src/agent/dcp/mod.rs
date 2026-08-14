//! Provider-neutral primitives for the Deterministic Context Pipeline (DCP).

mod agent_builder;
mod agent_definition;
#[cfg(test)]
mod agent_definition_tests;
mod agent_journal;
mod agent_store;
mod runner;
mod runner_support;
#[cfg(test)]
mod runner_tests;
mod runner_types;
mod runtime_policy;
mod types;
mod workspace_binding;

pub use agent_builder::build_agent_definition;
pub use agent_definition::{
    DCP_AGENT_API_VERSION, DcpAgentCapabilities, DcpAgentDefinition, DcpAgentExecution, DcpAgentLock, DcpAgentMetadata, DcpAgentOutput,
    DcpAgentOutputFormat, DcpAgentSpec, DcpRunState, DcpRunStatus, DcpRuntimePolicy, DcpSessionState, GitWorkspaceBinding,
    WorkspaceBinding,
};
pub use agent_journal::{DcpRunToolJournal, DcpToolJournalSnapshot, DcpToolOperation, DcpToolPhase};
pub use agent_store::{DcpRunBundle, DcpSessionBundle, DcpStore, SessionRunLease};
pub use runner_types::{DcpEventFormat, DcpEventSink, DcpRunRequest, DcpRunResult, DcpRuntime, DcpRuntimeEvent, DcpRuntimeOptions};
pub use types::{
    ContextCursor, ContextFrame, ContextLayer, ContextSegment, ProviderNeutralPart, TurnCursor, TurnReceipt, TurnRecord, TurnRecordKind,
    VisibilityRef,
};

use std::collections::BTreeMap;

use crate::core::identity::ContentHash;

pub trait ContextSource: Send + Sync {
    fn render(&self, cursor: ContextCursor) -> Result<Vec<ContextSegment>, DcpError>;
}

pub trait TurnJournal: Send + Sync {
    fn append(&self, expected: TurnCursor, record: TurnRecord) -> Result<TurnReceipt, DcpError>;
    fn load(&self, from: TurnCursor) -> Result<Vec<TurnRecord>, DcpError>;
}

/// Tool side-effect boundary used by every Agent execution runtime.
/// `before` must durably commit intent and start before execution. `after`
/// must durably commit the observed result. An UNKNOWN result permanently
/// blocks automatic continuation until an owner recovery decision exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolBoundaryAction {
    Execute,
    Replay { output: String, is_error: bool },
    Pause { reason: String },
}

pub trait ToolBoundaryJournal: Send + Sync {
    fn before(&self, call_id: &str, tool_name: &str, arguments_json: &str, at_ms: u64) -> Result<ToolBoundaryAction, String>;
    fn after(&self, call_id: &str, tool_name: &str, arguments_json: &str, output: &str, is_error: bool, at_ms: u64) -> Result<(), String>;
    fn mark_unknown(&self, call_id: &str, reason: &str, at_ms: u64) -> Result<(), String>;
    fn should_pause(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct ContextComposer {
    sources: Vec<Box<dyn ContextSource>>,
}

impl ContextComposer {
    pub fn push(&mut self, source: impl ContextSource + 'static) {
        self.sources.push(Box::new(source));
    }

    pub fn compose(&self, cursor: ContextCursor) -> Result<ContextFrame, DcpError> {
        let mut by_id = BTreeMap::new();
        for source in &self.sources {
            for segment in source.render(cursor)? {
                validate_segment(&segment)?;
                match by_id.get(&segment.stable_id) {
                    Some(existing) if existing != &segment => {
                        return Err(DcpError::SegmentCollision(segment.stable_id.to_string()));
                    }
                    Some(_) => {}
                    None => {
                        by_id.insert(segment.stable_id.clone(), segment);
                    }
                }
            }
        }
        let mut segments: Vec<_> = by_id.into_values().collect();
        segments.sort_by(|left, right| {
            (&left.layer, &left.order_key, &left.stable_id).cmp(&(&right.layer, &right.order_key, &right.stable_id))
        });
        let bytes = serde_json::to_vec(&segments).map_err(|error| DcpError::Codec(error.to_string()))?;
        Ok(ContextFrame { source_version: ContentHash::from_bytes(&bytes), segments })
    }
}

fn validate_segment(segment: &ContextSegment) -> Result<(), DcpError> {
    if segment.order_key.trim().is_empty() || segment.parts.is_empty() {
        return Err(DcpError::InvalidSegment(segment.stable_id.to_string()));
    }
    for part in &segment.parts {
        match part {
            ProviderNeutralPart::Text { text } if text.trim().is_empty() => {
                return Err(DcpError::InvalidSegment(segment.stable_id.to_string()));
            }
            ProviderNeutralPart::ToolCall { arguments_json, .. } => {
                serde_json::from_str::<serde_json::Value>(arguments_json)
                    .map_err(|_| DcpError::InvalidSegment(segment.stable_id.to_string()))?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum DcpError {
    #[error("DCP segment is invalid: {0}")]
    InvalidSegment(String),
    #[error("DCP stable segment collision: {0}")]
    SegmentCollision(String),
    #[error("DCP cursor conflict: expected {expected}, actual {actual}")]
    CursorConflict { expected: u64, actual: u64 },
    #[error("DCP codec: {0}")]
    Codec(String),
    #[error("DCP storage: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::identity::ResourceId;

    struct Source(Vec<ContextSegment>);

    impl ContextSource for Source {
        fn render(&self, _cursor: ContextCursor) -> Result<Vec<ContextSegment>, DcpError> {
            Ok(self.0.clone())
        }
    }

    fn segment(id: &str, layer: ContextLayer, order_key: &str, text: &str) -> ContextSegment {
        ContextSegment {
            stable_id: ResourceId::parse(id).unwrap(),
            layer,
            order_key: order_key.into(),
            visibility: VisibilityRef::Owner,
            parts: vec![ProviderNeutralPart::Text { text: text.into() }],
        }
    }

    #[test]
    fn composition_is_stable_ordered_and_deduplicated() {
        let one = segment("ctx_one", ContextLayer::Conversation, "002", "one");
        let two = segment("ctx_two", ContextLayer::Definition, "001", "two");
        let mut composer = ContextComposer::default();
        composer.push(Source(vec![one.clone(), two.clone()]));
        composer.push(Source(vec![one]));
        let frame = composer.compose(ContextCursor::default()).unwrap();
        assert_eq!(frame.segments.len(), 2);
        assert_eq!(frame.segments[0].stable_id, two.stable_id);
        assert_eq!(frame, composer.compose(ContextCursor::default()).unwrap());
    }

    #[test]
    fn same_id_with_different_content_is_rejected() {
        let mut composer = ContextComposer::default();
        composer.push(Source(vec![segment("ctx_same", ContextLayer::Memory, "001", "one")]));
        composer.push(Source(vec![segment("ctx_same", ContextLayer::Memory, "001", "two")]));
        assert!(matches!(composer.compose(ContextCursor::default()), Err(DcpError::SegmentCollision(_))));
    }
}
