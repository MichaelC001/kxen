//! Application-level composition root for Bot definitions, Runs and collaboration.

mod admission;
mod builder_flow;
mod dispatch;
mod error;
mod lifecycle;
mod routine_dispatch;
mod settlement;
mod types;

pub use error::BotSystemError;
pub use types::{ConversationMutation, DispatchReceipt, PostConversation, QueueRun, RoutineTickReport};

use std::path::{Path, PathBuf};

use crate::agent::capability::{CapabilityAvailability, CapabilityCatalog, CapabilityDescriptor, CapabilityKind};
use crate::core::identity::ResourceId;

#[cfg(test)]
use crate::bot::conversation::ConversationKind;
use crate::bot::conversation::{ConversationCommand, ConversationRepository, ConversationState, ConversationWrite};
use crate::bot::routine::RoutineRepository;
use crate::bot::run::{PermissionSnapshot, RunCommand, RunRepository, RunSpec};
use crate::bot::{BotLifecycle, BotRepository};

pub struct BotSystem {
    root: PathBuf,
    definitions: BotRepository,
    runs: RunRepository,
    conversations: ConversationRepository,
    routines: RoutineRepository,
    builder: crate::bot::builder::BuilderRepository,
    memory: crate::bot::memory::MemoryRepository,
    artifacts: crate::core::artifact::ArtifactStore,
    recovery: crate::core::recovery::RecoveryRegistry,
    capabilities: CapabilityCatalog,
}

impl BotSystem {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, BotSystemError> {
        let root = root.into();
        Ok(Self {
            definitions: BotRepository::new(&root),
            runs: RunRepository::new(&root),
            conversations: ConversationRepository::new(&root),
            routines: RoutineRepository::new(&root),
            builder: crate::bot::builder::BuilderRepository::new(&root),
            memory: crate::bot::memory::MemoryRepository::new(&root),
            artifacts: crate::core::artifact::ArtifactStore::new(&root),
            recovery: crate::core::recovery::RecoveryRegistry::new(&root),
            capabilities: runtime_catalog()?,
            root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn definitions(&self) -> &BotRepository {
        &self.definitions
    }

    pub fn runs(&self) -> &RunRepository {
        &self.runs
    }

    pub fn conversations(&self) -> &ConversationRepository {
        &self.conversations
    }

    pub fn routines(&self) -> &RoutineRepository {
        &self.routines
    }

    pub fn builder(&self) -> &crate::bot::builder::BuilderRepository {
        &self.builder
    }

    pub fn memory(&self) -> &crate::bot::memory::MemoryRepository {
        &self.memory
    }

    pub fn artifacts(&self) -> &crate::core::artifact::ArtifactStore {
        &self.artifacts
    }

    pub fn recovery(&self) -> &crate::core::recovery::RecoveryRegistry {
        &self.recovery
    }

    pub fn capabilities(&self) -> &CapabilityCatalog {
        &self.capabilities
    }

    pub fn queue_run(&self, request: QueueRun) -> Result<crate::bot::run::BotRunState, BotSystemError> {
        let QueueRun {
            run_id,
            bot_id,
            revision_id,
            trigger,
            input,
            conversation_id,
            task_id,
            budget_override,
            actor,
            trace,
            idempotency_key,
            at_ms,
        } = request;
        let bot = self.definitions.get(&bot_id)?;
        if bot.lifecycle != BotLifecycle::Active {
            return Err(BotSystemError::Rejected(format!("Bot is {:?}", bot.lifecycle)));
        }
        let revision = match revision_id {
            Some(revision_id) => bot
                .revisions
                .values()
                .find(|revision| revision.revision_id == revision_id)
                .ok_or_else(|| BotSystemError::Rejected("requested Bot revision is unavailable".into()))?,
            None => bot.current_revision().ok_or_else(|| BotSystemError::Rejected("Bot has no published revision".into()))?,
        };
        self.capabilities.resolve(&revision.definition.capabilities).map_err(|error| BotSystemError::Rejected(error.to_string()))?;
        revision.definition.validate_input(&input).map_err(|error| BotSystemError::Rejected(error.to_string()))?;
        let budget = effective_budget(&revision.definition, budget_override.as_ref());
        budget.validate().map_err(BotSystemError::Rejected)?;
        if let Some(conversation_id) = &conversation_id {
            let conversation = self.conversations.get(conversation_id)?;
            if conversation.lifecycle != crate::bot::conversation::ConversationLifecycle::Active {
                return Err(BotSystemError::Rejected(format!("Conversation is {:?}", conversation.lifecycle)));
            }
            if !conversation.members.get(&bot_id).is_some_and(|member| member.active) {
                return Err(BotSystemError::Rejected(format!("Bot is not an active Conversation member: {bot_id}")));
            }
            if let Some(task_id) = &task_id {
                let task = conversation
                    .tasks
                    .get(task_id)
                    .ok_or_else(|| BotSystemError::Rejected(format!("Conversation task is unavailable: {task_id}")))?;
                if task.owner_bot_id != bot_id || task.status.is_terminal() {
                    return Err(BotSystemError::Rejected("BotRun task must be nonterminal and owned by the Run Bot".into()));
                }
            }
        } else if task_id.is_some() {
            return Err(BotSystemError::Rejected("task-bound BotRun requires a Conversation".into()));
        }
        let spec = RunSpec {
            run_id: run_id.clone(),
            bot_id,
            revision_id: revision.revision_id.clone(),
            revision_hash: revision.content_hash.clone(),
            mrm_role: revision.definition.mrm_role.clone(),
            trigger,
            input,
            conversation_id,
            task_id,
            permission: PermissionSnapshot {
                capabilities: revision.definition.capabilities.clone(),
                resources: revision.definition.resources.clone(),
                approval: revision.definition.approval,
                budget,
            },
        };
        let command = RunCommand::Queue { spec: Box::new(spec), at_ms };
        Ok(self.runs.execute(crate::bot::run::RunWrite { run_id, expected_version: 0, idempotency_key, actor, trace, command })?)
    }

    pub fn mutate_conversation(&self, request: ConversationMutation) -> Result<ConversationState, BotSystemError> {
        self.admit_conversation_command(&request.conversation_id, &request.command, &request.actor)?;
        Ok(self.conversations.execute(ConversationWrite {
            conversation_id: request.conversation_id,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            actor: request.actor,
            trace: request.trace,
            command: request.command,
        })?)
    }

    pub fn post_conversation(&self, request: PostConversation) -> Result<ConversationState, BotSystemError> {
        let state = self.conversations.get(&request.conversation_id)?;
        self.admit_post(&state, &request.actor, &request.message)?;
        self.mutate_conversation(ConversationMutation {
            conversation_id: request.conversation_id,
            expected_version: request.expected_version,
            actor: request.actor,
            command: ConversationCommand::Post { message: Box::new(request.message), task: request.task, at_ms: request.at_ms },
            trace: request.trace,
            idempotency_key: request.idempotency_key,
        })
    }
}

fn runtime_catalog() -> Result<CapabilityCatalog, BotSystemError> {
    let mut catalog = CapabilityCatalog::default();
    for tool in crate::agent::tools_spec::core_tools().into_iter().chain(crate::agent::tools_spec::deferred_tools()) {
        let id = ResourceId::parse(tool.function.name.clone()).map_err(BotSystemError::InvalidId)?;
        let availability =
            if bot_runtime_tool(&tool.function.name) { CapabilityAvailability::Available } else { CapabilityAvailability::Unavailable };
        catalog
            .register(CapabilityDescriptor {
                id,
                kind: CapabilityKind::Tool,
                display_name: tool.function.description.clone(),
                availability,
                requires_approval: matches!(tool.function.name.as_str(), "edit" | "write" | "delete"),
            })
            .map_err(|error| BotSystemError::Rejected(error.to_string()))?;
    }
    for name in crate::bot::builder::BUILDER_CAPABILITIES {
        catalog
            .register(CapabilityDescriptor {
                id: ResourceId::parse(*name).map_err(BotSystemError::InvalidId)?,
                kind: CapabilityKind::Tool,
                display_name: (*name).into(),
                availability: CapabilityAvailability::Unavailable,
                requires_approval: false,
            })
            .map_err(|error| BotSystemError::Rejected(error.to_string()))?;
    }
    for tool in crate::bot::tools::definitions() {
        catalog
            .register(CapabilityDescriptor {
                id: ResourceId::parse(tool.function.name.clone()).map_err(BotSystemError::InvalidId)?,
                kind: CapabilityKind::Tool,
                display_name: tool.function.description.clone(),
                availability: CapabilityAvailability::Available,
                requires_approval: false,
            })
            .map_err(|error| BotSystemError::Rejected(error.to_string()))?;
    }
    Ok(catalog)
}

fn bot_runtime_tool(name: &str) -> bool {
    // LSP navigation can return cross-file locations outside a granted path.
    // It remains unavailable to Bots until its result surface is ACL-aware.
    matches!(name, "read" | "edit" | "write" | "delete" | "glob" | "grep" | "webfetch" | "websearch" | "tool_search" | "todo")
}

pub(super) fn effective_budget(
    definition: &crate::bot::BotDefinition,
    override_budget: Option<&crate::agent::runtime::ExecutionBudget>,
) -> crate::agent::runtime::ExecutionBudget {
    let mut definition_budget = definition.budget.clone();
    definition_budget.max_turns =
        Some(definition_budget.max_turns.map_or(definition.context.max_run_turns, |value| value.min(definition.context.max_run_turns)));
    let requested = override_budget.map_or_else(|| definition_budget.clone(), |value| definition_budget.most_restrictive(value));
    requested.most_restrictive(&crate::agent::runtime::ExecutionBudget {
        max_child_tasks: Some(32),
        max_delegation_depth: Some(8),
        max_message_hops: Some(32),
        ..Default::default()
    })
}

pub fn stable_idempotency(prefix: &str, parts: &[&str]) -> Result<crate::core::identity::IdempotencyKey, String> {
    let id = crate::bot::deterministic_id(prefix, parts)?;
    crate::core::identity::IdempotencyKey::parse(id.to_string())
}

#[cfg(test)]
mod limits_tests;
#[cfg(test)]
mod tests;
