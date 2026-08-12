use crate::bot::memory::{MemoryCommand, MemoryItem, MemoryKind, MemoryWrite};
use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, ResourceId, TraceContext};

use super::helpers;

pub(super) fn execute(system: &crate::bot::system::BotSystem, run_id: &ResourceId, args: &serde_json::Value) -> Result<String, String> {
    let run = helpers::run(system, run_id)?;
    let action = helpers::required(args, "action")?;
    let bot = system.definitions().get(&run.spec.bot_id).map_err(|error| error.to_string())?;
    let revision = bot
        .revisions
        .values()
        .find(|revision| revision.revision_id == run.spec.revision_id)
        .ok_or("BotRun Memory revision is unavailable")?;
    if !revision.definition.memory.enabled {
        return Err("Bot Memory is disabled by the immutable revision".into());
    }
    let current = system.memory().get(&run.spec.bot_id).map_err(|error| error.to_string())?;
    if action == "list" {
        return serde_json::to_string(&current).map_err(|error| error.to_string());
    }
    let now = crate::core::shared::now_ms();
    let command = match action {
        "propose_create" => {
            if current.items.len() >= revision.definition.memory.max_items as usize {
                return Err("Bot Memory item limit reached".into());
            }
            let kind: MemoryKind =
                serde_json::from_value(args.get("kind").cloned().ok_or("missing kind")?).map_err(|error| error.to_string())?;
            MemoryCommand::Create {
                item: MemoryItem {
                    item_id: helpers::stable_id("bmem", run_id, args)?,
                    kind,
                    content: helpers::required(args, "content")?.into(),
                    provenance: AggregateRef { kind: AggregateKind::BotRun, id: run_id.clone() },
                    version: 1,
                    created_at_ms: now,
                    updated_at_ms: now,
                },
            }
        }
        "propose_revise" => {
            let item_id = ResourceId::parse(helpers::required(args, "item_id")?)?;
            let item = current.items.get(&item_id).ok_or_else(|| format!("Memory item not found: {item_id}"))?;
            let expected = args.get("expected_item_version").and_then(serde_json::Value::as_u64).unwrap_or(item.version);
            MemoryCommand::Revise {
                item_id,
                expected_item_version: expected,
                content: helpers::required(args, "content")?.into(),
                at_ms: now,
            }
        }
        "propose_remove" => {
            let item_id = ResourceId::parse(helpers::required(args, "item_id")?)?;
            let item = current.items.get(&item_id).ok_or_else(|| format!("Memory item not found: {item_id}"))?;
            let expected = args.get("expected_item_version").and_then(serde_json::Value::as_u64).unwrap_or(item.version);
            MemoryCommand::Remove { item_id, expected_item_version: expected, at_ms: now }
        }
        _ => return Err(format!("unknown bot_memory action: {action}")),
    };
    let state = system
        .memory()
        .execute(MemoryWrite {
            bot_id: run.spec.bot_id.clone(),
            expected_version: current.event_version,
            idempotency_key: helpers::stable_key("bot_memory", run_id, args)?,
            actor: ActorRef::Bot { id: run.spec.bot_id },
            trace: TraceContext::default(),
            command,
        })
        .map_err(|error| error.to_string())?;
    serde_json::to_string(&state).map_err(|error| error.to_string())
}
