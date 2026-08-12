use std::path::{Path, PathBuf};

use crate::core::event_store::{EventEntry, EventStore};
use crate::core::identity::{
    ActorRef, AggregateKind, AggregateRef, ContentHash, IdempotencyKey, ResourceId, SchemaVersion, Sequence, TraceContext,
};

use super::BotError;
use super::definition::BotDefinition;
use super::events::BotEvent;
use super::ids::deterministic_id;
use super::projection;
use super::types::{BotDefinitionRevision, BotDraft, BotLifecycle, BotState, BotSummary};

const BOT_SCHEMA_VERSION: u32 = 1;

pub struct BotRepository {
    root: PathBuf,
}

impl BotRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create(&self, command: CreateBot<'_>) -> Result<BotState, BotError> {
        command.definition.validate_draft()?;
        let loaded = self.prepare(command.bot_id, 0, &command.idempotency_key, true)?;
        let draft = BotDraft {
            version: 1,
            content_hash: command.definition.content_hash()?,
            definition: command.definition.clone(),
            updated_at_ms: command.at_ms,
        };
        let event = BotEvent::Created { bot_id: command.bot_id.clone(), draft, at_ms: command.at_ms };
        self.append_loaded(loaded, command.actor, command.trace, command.idempotency_key, event)
    }

    pub fn get(&self, bot_id: &ResourceId) -> Result<BotState, BotError> {
        self.load(bot_id)?.state.ok_or_else(|| BotError::NotFound(bot_id.to_string()))
    }

    pub fn list(&self, include_trashed: bool) -> Result<Vec<BotSummary>, BotError> {
        let definitions = self.root.join("definitions");
        let mut summaries = Vec::new();
        match std::fs::read_dir(definitions) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    if !entry.file_type()?.is_dir() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let bot_id = ResourceId::parse(name).map_err(BotError::InvalidDefinition)?;
                    let state = self.get(&bot_id)?;
                    if include_trashed || state.lifecycle != BotLifecycle::Trashed {
                        summaries.push(BotSummary::from(&state));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        summaries.sort_by(|left, right| left.bot_id.cmp(&right.bot_id));
        Ok(summaries)
    }

    pub fn replace_draft(&self, command: ReplaceDraft<'_>) -> Result<BotState, BotError> {
        command.definition.validate_draft()?;
        let loaded = self.prepare(command.bot_id, command.expected_event_version, &command.idempotency_key, false)?;
        let state = loaded.state.as_ref().expect("required by require_version");
        if !matches!(state.lifecycle, BotLifecycle::Draft | BotLifecycle::Active | BotLifecycle::Paused) {
            return Err(BotError::LifecycleRejected(format!("{:?} -> replace_draft", state.lifecycle)));
        }
        let actual_draft = state.draft.as_ref().map_or(0, |draft| draft.version);
        if actual_draft != command.expected_draft_version {
            return Err(BotError::VersionConflict(format!("expected draft {}, actual {actual_draft}", command.expected_draft_version)));
        }
        let draft = BotDraft {
            version: state.draft_version_counter.checked_add(1).ok_or_else(|| BotError::InvalidEvent("draft version overflow".into()))?,
            content_hash: command.definition.content_hash()?,
            definition: command.definition.clone(),
            updated_at_ms: command.at_ms,
        };
        self.append_loaded(
            loaded,
            command.actor,
            command.trace,
            command.idempotency_key,
            BotEvent::DraftReplaced { draft, at_ms: command.at_ms },
        )
    }

    pub fn publish(&self, command: PublishBot<'_>) -> Result<BotState, BotError> {
        let loaded = self.prepare(command.bot_id, command.expected_event_version, &command.idempotency_key, false)?;
        let state = loaded.state.as_ref().expect("required by require_version");
        if !matches!(state.lifecycle, BotLifecycle::Draft | BotLifecycle::Active | BotLifecycle::Paused) {
            return Err(BotError::LifecycleRejected(format!("{:?} -> publish", state.lifecycle)));
        }
        let draft = state.draft.as_ref().ok_or_else(|| BotError::VersionConflict("no unpublished draft".into()))?;
        if draft.version != command.expected_draft_version || draft.content_hash != *command.expected_content_hash {
            return Err(BotError::VersionConflict("draft version or content hash changed".into()));
        }
        draft.definition.validate_publish()?;
        let revision_number = u64::try_from(state.revisions.len())
            .map_err(|_| BotError::InvalidEvent("revision count overflow".into()))?
            .checked_add(1)
            .ok_or_else(|| BotError::InvalidEvent("revision count overflow".into()))?;
        let revision = BotDefinitionRevision {
            revision_id: deterministic_id("brev", &[command.idempotency_key.as_str()]).map_err(BotError::InvalidDefinition)?,
            revision_number,
            definition: draft.definition.clone(),
            content_hash: draft.content_hash.clone(),
            created_at_ms: command.at_ms,
        };
        self.append_loaded(
            loaded,
            command.actor,
            command.trace,
            command.idempotency_key,
            BotEvent::RevisionPublished { revision, at_ms: command.at_ms },
        )
    }

    pub fn change_lifecycle(&self, command: ChangeLifecycle<'_>) -> Result<BotState, BotError> {
        let loaded = self.prepare(command.bot_id, command.expected_event_version, &command.idempotency_key, false)?;
        let event = match command.change {
            LifecycleChange::Pause => BotEvent::Paused { at_ms: command.at_ms },
            LifecycleChange::Resume => BotEvent::Resumed { at_ms: command.at_ms },
            LifecycleChange::Archive => BotEvent::Archived { at_ms: command.at_ms },
            LifecycleChange::Trash => BotEvent::Trashed { at_ms: command.at_ms },
            LifecycleChange::Restore => BotEvent::Restored { at_ms: command.at_ms },
            LifecycleChange::Block(reason) => {
                if reason.trim().is_empty() {
                    return Err(BotError::LifecycleRejected("blocked reason cannot be empty".into()));
                }
                BotEvent::Blocked { reason: reason.to_string(), at_ms: command.at_ms }
            }
            LifecycleChange::ClearRecovery => BotEvent::RecoveryCleared { at_ms: command.at_ms },
        };
        self.append_loaded(loaded, command.actor, command.trace, command.idempotency_key, event)
    }

    fn append_loaded(
        &self,
        loaded: LoadedBot,
        actor: ActorRef,
        trace: TraceContext,
        idempotency_key: IdempotencyKey,
        event: BotEvent,
    ) -> Result<BotState, BotError> {
        let mut projected = loaded.state.clone();
        projection::apply(&mut projected, &event)?;
        let event_id = deterministic_id("bevt", &[idempotency_key.as_str()]).map_err(BotError::InvalidDefinition)?;
        loaded.store.append(loaded.last_seq, idempotency_key, actor, trace, vec![EventEntry { event_id, payload: event }])?;
        self.load_store(&loaded.bot_id, &loaded.store)?.state.ok_or_else(|| BotError::InvalidEvent("append produced no state".into()))
    }

    fn prepare(
        &self,
        bot_id: &ResourceId,
        expected: u64,
        idempotency_key: &IdempotencyKey,
        allow_missing: bool,
    ) -> Result<LoadedBot, BotError> {
        let mut loaded = self.load(bot_id)?;
        if let Some(index) = loaded.batches.iter().position(|batch| &batch.idempotency_key == idempotency_key) {
            loaded.state = replay(&loaded.batches[..index])?.0;
            return Ok(loaded);
        }
        let actual = loaded.state.as_ref().map_or(0, |state| state.event_version);
        if (!allow_missing && loaded.state.is_none()) || actual != expected {
            return Err(if loaded.state.is_none() {
                BotError::NotFound(bot_id.to_string())
            } else {
                BotError::VersionConflict(format!("expected event {expected}, actual {actual}"))
            });
        }
        Ok(loaded)
    }

    fn load(&self, bot_id: &ResourceId) -> Result<LoadedBot, BotError> {
        let store = self.store(bot_id)?;
        self.load_store(bot_id, &store)
    }

    fn load_store(&self, bot_id: &ResourceId, store: &EventStore<BotEvent>) -> Result<LoadedBot, BotError> {
        let batches = store.load()?;
        let (state, last_seq) = replay(&batches)?;
        Ok(LoadedBot { bot_id: bot_id.clone(), store: self.store(bot_id)?, state, last_seq, batches })
    }

    fn store(&self, bot_id: impl AsRef<str>) -> Result<EventStore<BotEvent>, BotError> {
        let bot_id = ResourceId::parse(bot_id.as_ref()).map_err(BotError::InvalidDefinition)?;
        Ok(EventStore::new(
            self.bot_path(&bot_id),
            AggregateRef { kind: AggregateKind::Bot, id: bot_id },
            SchemaVersion::new(BOT_SCHEMA_VERSION).expect("positive schema version"),
        ))
    }

    fn bot_path(&self, bot_id: &ResourceId) -> PathBuf {
        self.root.join("definitions").join(bot_id.as_str())
    }
}

struct LoadedBot {
    bot_id: ResourceId,
    store: EventStore<BotEvent>,
    state: Option<BotState>,
    last_seq: Sequence,
    batches: Vec<crate::core::event_store::EventBatch<BotEvent>>,
}

fn replay(batches: &[crate::core::event_store::EventBatch<BotEvent>]) -> Result<(Option<BotState>, Sequence), BotError> {
    let mut state = None;
    let mut last_seq = Sequence(0);
    for batch in batches {
        last_seq = batch.last_seq()?;
        for event in &batch.events {
            projection::apply(&mut state, &event.payload)?;
        }
    }
    Ok((state, last_seq))
}

pub struct CreateBot<'a> {
    pub bot_id: &'a ResourceId,
    pub definition: &'a BotDefinition,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub idempotency_key: IdempotencyKey,
    pub at_ms: u64,
}

pub struct ReplaceDraft<'a> {
    pub bot_id: &'a ResourceId,
    pub expected_event_version: u64,
    pub expected_draft_version: u64,
    pub definition: &'a BotDefinition,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub idempotency_key: IdempotencyKey,
    pub at_ms: u64,
}

pub struct PublishBot<'a> {
    pub bot_id: &'a ResourceId,
    pub expected_event_version: u64,
    pub expected_draft_version: u64,
    pub expected_content_hash: &'a ContentHash,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub idempotency_key: IdempotencyKey,
    pub at_ms: u64,
}

pub struct ChangeLifecycle<'a> {
    pub bot_id: &'a ResourceId,
    pub expected_event_version: u64,
    pub change: LifecycleChange<'a>,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub idempotency_key: IdempotencyKey,
    pub at_ms: u64,
}

pub enum LifecycleChange<'a> {
    Pause,
    Resume,
    Archive,
    Trash,
    Restore,
    Block(&'a str),
    ClearRecovery,
}
