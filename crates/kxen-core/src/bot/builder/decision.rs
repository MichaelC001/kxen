use crate::core::identity::{ActorRef, SystemActor};

use super::BuilderError;
use super::command::BuilderCommand;
use super::events::BuilderEvent;
use super::types::{BuilderDraft, BuilderLifecycle, BuilderState};

pub fn decide(state: Option<&BuilderState>, actor: &ActorRef, command: BuilderCommand) -> Result<Vec<BuilderEvent>, BuilderError> {
    match command {
        BuilderCommand::Start { builder_session_id, bot_id, user_goal, at_ms } => {
            require_owner(actor)?;
            if state.is_some() || user_goal.trim().is_empty() {
                return Err(BuilderError::Rejected("BuilderSession already exists or goal is empty".into()));
            }
            Ok(vec![BuilderEvent::Started { builder_session_id, bot_id, user_goal, at_ms }])
        }
        command => decide_existing(state.ok_or_else(|| BuilderError::NotFound("uninitialized".into()))?, actor, command),
    }
}

fn decide_existing(state: &BuilderState, actor: &ActorRef, command: BuilderCommand) -> Result<Vec<BuilderEvent>, BuilderError> {
    if state.lifecycle != BuilderLifecycle::Active {
        return Err(BuilderError::Rejected(format!("BuilderSession is {:?}", state.lifecycle)));
    }
    let event = match command {
        BuilderCommand::Start { .. } => unreachable!(),
        BuilderCommand::AppendMessage { message, at_ms } => {
            if message.text.trim().is_empty() || &message.actor != actor || !is_owner_or_self_builder(state, actor) {
                return Err(BuilderError::Rejected("self-builder message actor or content is invalid".into()));
            }
            if state.messages.last().is_some_and(|pending| {
                pending.actor == ActorRef::Owner
                    && state.draft.as_ref().and_then(|draft| draft.source_message_id.as_ref()) != Some(&pending.message_id)
            }) {
                return Err(BuilderError::Rejected("previous Owner message is still awaiting a Builder reply".into()));
            }
            BuilderEvent::MessageAppended { message, at_ms }
        }
        BuilderCommand::ApplyTurn { source_message_id, message, expected_draft_version, definition, at_ms } => {
            require_self_builder(state, actor)?;
            if message.actor != *actor || message.text.trim().is_empty() {
                return Err(BuilderError::Rejected("self-builder reply actor or content is invalid".into()));
            }
            let source = state.messages.last().ok_or_else(|| BuilderError::Rejected("Builder turn source message is missing".into()))?;
            if source.message_id != source_message_id || source.actor != ActorRef::Owner {
                return Err(BuilderError::Rejected("Builder turn must answer the latest Owner message".into()));
            }
            let mut events = vec![BuilderEvent::MessageAppended { message, at_ms }];
            if let Some(definition) = definition {
                definition.validate_draft().map_err(|error| BuilderError::Rejected(error.to_string()))?;
                let actual = state.draft.as_ref().map_or(0, |draft| draft.version);
                if actual != expected_draft_version {
                    return Err(BuilderError::VersionConflict { expected: expected_draft_version, actual });
                }
                let draft = BuilderDraft {
                    version: actual.checked_add(1).ok_or_else(|| BuilderError::Rejected("draft version overflow".into()))?,
                    source_message_id: Some(source_message_id),
                    content_hash: definition.content_hash().map_err(|error| BuilderError::Rejected(error.to_string()))?,
                    definition: *definition,
                    updated_at_ms: at_ms,
                };
                events.push(BuilderEvent::DraftReplaced { draft: Box::new(draft), at_ms });
            }
            return Ok(events);
        }
        BuilderCommand::ReplaceDraft { expected_draft_version, source_message_id, definition, at_ms } => {
            if !is_owner_or_self_builder(state, actor) {
                return Err(BuilderError::Rejected("only Owner or the target Bot self-builder can patch draft".into()));
            }
            definition.validate_draft().map_err(|error| BuilderError::Rejected(error.to_string()))?;
            let actual = state.draft.as_ref().map_or(0, |draft| draft.version);
            if actual != expected_draft_version {
                return Err(BuilderError::VersionConflict { expected: expected_draft_version, actual });
            }
            let draft = BuilderDraft {
                version: actual.checked_add(1).ok_or_else(|| BuilderError::Rejected("draft version overflow".into()))?,
                source_message_id,
                content_hash: definition.content_hash().map_err(|error| BuilderError::Rejected(error.to_string()))?,
                definition: *definition,
                updated_at_ms: at_ms,
            };
            BuilderEvent::DraftReplaced { draft: Box::new(draft), at_ms }
        }
        BuilderCommand::RecordGrant { grant, at_ms } => {
            require_owner(actor)?;
            let draft = state.draft.as_ref().ok_or_else(|| BuilderError::Rejected("draft is missing".into()))?;
            if grant.draft_hash != draft.content_hash || grant.reason.trim().is_empty() {
                return Err(BuilderError::Rejected("grant must bind exact draft and reason".into()));
            }
            BuilderEvent::PermissionGranted { grant, at_ms }
        }
        BuilderCommand::RecordValidation { report, at_ms } => {
            require_runtime(actor)?;
            if state.draft.as_ref().map(|draft| &draft.content_hash) != Some(&report.draft_hash) {
                return Err(BuilderError::Rejected("validation report is stale".into()));
            }
            BuilderEvent::ValidationRecorded { report, at_ms }
        }
        BuilderCommand::LinkTestRun { run_id, draft_hash, at_ms } => {
            if !is_owner_or_self_builder(state, actor) || state.active_test_run_id.is_some() {
                return Err(BuilderError::Rejected("test run cannot be linked".into()));
            }
            if state.draft.as_ref().map(|draft| &draft.content_hash) != Some(&draft_hash) {
                return Err(BuilderError::Rejected("test run draft hash is stale".into()));
            }
            BuilderEvent::TestRunLinked { run_id, draft_hash, at_ms }
        }
        BuilderCommand::RecordTestEvidence { evidence, at_ms } => {
            require_runtime(actor)?;
            if state.active_test_run_id.as_ref() != Some(&evidence.run_id) {
                return Err(BuilderError::Rejected("test evidence Run is stale".into()));
            }
            BuilderEvent::TestEvidenceRecorded { evidence, at_ms }
        }
        BuilderCommand::Cancel { at_ms } => {
            require_owner(actor)?;
            BuilderEvent::Canceled { at_ms }
        }
        BuilderCommand::Block { reason, at_ms } => {
            if reason.trim().is_empty() {
                return Err(BuilderError::Rejected("blocked reason cannot be empty".into()));
            }
            BuilderEvent::Blocked { reason, at_ms }
        }
    };
    Ok(vec![event])
}

fn is_owner_or_self_builder(state: &BuilderState, actor: &ActorRef) -> bool {
    actor == &ActorRef::Owner || matches!(actor, ActorRef::Bot { id } if id == &state.bot_id)
}

fn require_owner(actor: &ActorRef) -> Result<(), BuilderError> {
    if actor == &ActorRef::Owner { Ok(()) } else { Err(BuilderError::Rejected("owner action required".into())) }
}

fn require_self_builder(state: &BuilderState, actor: &ActorRef) -> Result<(), BuilderError> {
    if matches!(actor, ActorRef::Bot { id } if id == &state.bot_id) {
        Ok(())
    } else {
        Err(BuilderError::Rejected("target Bot self-builder actor required".into()))
    }
}

fn require_runtime(actor: &ActorRef) -> Result<(), BuilderError> {
    if actor == &(ActorRef::System { actor: SystemActor::Runtime }) {
        Ok(())
    } else {
        Err(BuilderError::Rejected("runtime actor required".into()))
    }
}
