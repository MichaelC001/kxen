use std::collections::BTreeSet;

use crate::bot::builder::{BuilderCommand, BuilderWrite, ValidationContext};
use crate::bot::{PublishBot, ReplaceDraft};
use crate::core::identity::{ActorRef, ContentHash, IdempotencyKey, ResourceId, SystemActor, TraceContext};

use super::{BotSystem, BotSystemError};

impl BotSystem {
    pub fn queue_builder_test(
        &self,
        builder_session_id: &ResourceId,
        run_id: ResourceId,
        input: Vec<crate::agent::dcp::ProviderNeutralPart>,
        idempotency_key: IdempotencyKey,
        at_ms: u64,
    ) -> Result<crate::bot::run::BotRunState, BotSystemError> {
        let builder = self.builder.get(builder_session_id)?;
        let draft = builder.draft.as_ref().ok_or_else(|| BotSystemError::Rejected("Builder draft is missing".into()))?;
        self.capabilities.resolve(&draft.definition.capabilities).map_err(|error| BotSystemError::Rejected(error.to_string()))?;
        let revision_id = crate::bot::ids::deterministic_id("btestrev", &[builder_session_id.as_str(), draft.content_hash.as_str()])
            .map_err(BotSystemError::InvalidId)?;
        let spec = crate::bot::run::RunSpec {
            run_id: run_id.clone(),
            bot_id: builder.bot_id.clone(),
            revision_id,
            revision_hash: draft.content_hash.clone(),
            mrm_role: draft.definition.mrm_role.clone(),
            trigger: crate::bot::run::RunTrigger {
                kind: crate::bot::run::RunTriggerKind::BuilderTest,
                source_id: Some(builder_session_id.clone()),
                occurrence_id: None,
            },
            input,
            conversation_id: None,
            task_id: None,
            permission: crate::bot::run::PermissionSnapshot {
                capabilities: draft.definition.capabilities.clone(),
                resources: draft.definition.resources.clone(),
                approval: draft.definition.approval,
                budget: super::effective_budget(&draft.definition, None),
            },
        };
        let run = self.runs.execute(crate::bot::run::RunWrite {
            run_id: run_id.clone(),
            expected_version: 0,
            idempotency_key: idempotency_key.clone(),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: crate::bot::run::RunCommand::Queue { spec: Box::new(spec), at_ms },
        })?;
        self.builder.execute(BuilderWrite {
            builder_session_id: builder_session_id.clone(),
            expected_version: builder.event_version,
            idempotency_key: IdempotencyKey::parse(
                crate::bot::ids::deterministic_id("idem", &[idempotency_key.as_str(), "link"])
                    .map_err(BotSystemError::InvalidId)?
                    .to_string(),
            )
            .map_err(BotSystemError::InvalidId)?,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: BuilderCommand::LinkTestRun { run_id, draft_hash: draft.content_hash.clone(), at_ms },
        })?;
        Ok(run)
    }

    pub fn sync_builder_draft(
        &self,
        builder_session_id: &ResourceId,
        idempotency_key: IdempotencyKey,
        at_ms: u64,
    ) -> Result<crate::bot::BotState, BotSystemError> {
        let builder = self.builder.get(builder_session_id)?;
        let draft = builder.draft.ok_or_else(|| BotSystemError::Rejected("Builder draft is missing".into()))?;
        let bot = self.definitions.get(&builder.bot_id)?;
        let bot_draft_version = bot.draft.as_ref().map_or(0, |value| value.version);
        Ok(self.definitions.replace_draft(ReplaceDraft {
            bot_id: &builder.bot_id,
            expected_event_version: bot.event_version,
            expected_draft_version: bot_draft_version,
            definition: &draft.definition,
            actor: ActorRef::System { actor: SystemActor::Builder },
            trace: TraceContext::default(),
            idempotency_key,
            at_ms,
        })?)
    }

    pub fn validate_builder(
        &self,
        builder_session_id: &ResourceId,
        mrm_roles: &BTreeSet<ResourceId>,
        idempotency_key: IdempotencyKey,
        at_ms: u64,
    ) -> Result<crate::bot::builder::BuilderState, BotSystemError> {
        let builder = self.builder.get(builder_session_id)?;
        let draft = builder.draft.as_ref().ok_or_else(|| BotSystemError::Rejected("Builder draft is missing".into()))?;
        let report_id = crate::bot::ids::deterministic_id("review", &[builder_session_id.as_str(), draft.content_hash.as_str()])
            .map_err(BotSystemError::InvalidId)?;
        let report = crate::bot::builder::validate(
            report_id,
            &draft.definition,
            ValidationContext {
                catalog: &self.capabilities,
                mrm_roles,
                grant: builder.grants.iter().rev().find(|grant| grant.draft_hash == draft.content_hash),
                tests: &builder.tests,
            },
            at_ms,
        )
        .map_err(BotSystemError::Rejected)?;
        Ok(self.builder.execute(BuilderWrite {
            builder_session_id: builder_session_id.clone(),
            expected_version: builder.event_version,
            idempotency_key,
            actor: ActorRef::System { actor: SystemActor::Builder },
            trace: TraceContext::default(),
            command: BuilderCommand::RecordValidation { report, at_ms },
        })?)
    }

    pub fn publish_validated_builder(
        &self,
        builder_session_id: &ResourceId,
        review_hash: &ContentHash,
        idempotency_key: IdempotencyKey,
        at_ms: u64,
    ) -> Result<crate::bot::BotState, BotSystemError> {
        let builder = self.builder.get(builder_session_id)?;
        let draft = builder.draft.as_ref().ok_or_else(|| BotSystemError::Rejected("Builder draft is missing".into()))?;
        let report = builder.current_report().ok_or_else(|| BotSystemError::Rejected("current Builder validation is missing".into()))?;
        if !report.publish_eligible || &report.draft_hash != review_hash || &draft.content_hash != review_hash {
            return Err(BotSystemError::Rejected("publish requires the exact eligible review hash".into()));
        }
        let bot = self.definitions.get(&builder.bot_id)?;
        let bot_draft = bot.draft.as_ref().ok_or_else(|| BotSystemError::Rejected("Bot draft is missing".into()))?;
        if bot_draft.content_hash != draft.content_hash {
            return Err(BotSystemError::Rejected("Builder and Bot drafts are not synchronized".into()));
        }
        Ok(self.definitions.publish(PublishBot {
            bot_id: &builder.bot_id,
            expected_event_version: bot.event_version,
            expected_draft_version: bot_draft.version,
            expected_content_hash: &bot_draft.content_hash,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key,
            at_ms,
        })?)
    }
}
