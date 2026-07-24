//! goal RPC 方法（goal.{list,create,activate,pause,resume,complete,cancel,get}）。
//! 状态迁移成功后 publish GoalUpdate（Dock goal 面板实时刷新，此前变体只有定义无发布点）。

use kxen_app::core::event::Event;
use kxen_app::core::goal::{Goal, GoalBudget, GoalContract};
use kxen_app::core::paths;
use serde_json::{json, Value};

fn dir() -> std::path::PathBuf {
    paths::goals_dir()
}

fn to_json(goal: &Goal) -> Value {
    json!({
        "id": goal.id,
        "status": format!("{:?}", goal.status).to_lowercase(),
        "objective": goal.contract.objective,
        "completion_criteria": goal.contract.completion_criteria,
        "constraints": goal.contract.constraints,
        "budget": goal.contract.budget,
        "turns_used": goal.turns_used,
        "tokens_used": goal.tokens_used,
        "consecutive_blocks": goal.consecutive_blocks,
        "block_reason": goal.block_reason,
        "verification_evidence": goal.verification_evidence,
    })
}

pub fn call(method: &str, params: Value, bus: &kxen_app::core::event::EventBus) -> Result<Value, String> {
    match method {
        "goal.list" => {
            let goals = Goal::list(&dir());
            Ok(json!(goals.iter().map(to_json).collect::<Vec<_>>()))
        }
        "goal.focus" => Ok(Goal::focus_for(&dir(), params.get("session_id").and_then(Value::as_str)).map(|g| to_json(&g)).unwrap_or(Value::Null)),
        "goal.create" => {
            let contract = GoalContract {
                objective: params.get("objective").and_then(Value::as_str).ok_or("missing objective")?.to_string(),
                completion_criteria: params.get("completion_criteria").and_then(Value::as_str).ok_or("missing completion_criteria")?.to_string(),
                constraints: params.get("constraints").and_then(Value::as_str).map(String::from),
                budget: GoalBudget {
                    tokens: params.pointer("/budget/tokens").and_then(Value::as_u64),
                    turns: params.pointer("/budget/turns").and_then(Value::as_u64).map(|n| n as u32),
                    wall_clock_ms: params.pointer("/budget/wall_clock_ms").and_then(Value::as_u64),
                },
            };
            let id = kxen_app::core::ids::new_id("goal");
            let mut goal = Goal::create(contract, id).map_err(|e| e.to_string())?;
            goal.session_id = params.get("session_id").and_then(Value::as_str).map(String::from);
            goal.save(&dir()).map_err(|e| e.to_string())?;
            publish(bus, &goal);
            Ok(to_json(&goal))
        }
        "goal.get" => {
            let goal = load(params.get("id").and_then(Value::as_str).ok_or("missing id")?)?;
            Ok(to_json(&goal))
        }
        "goal.activate" => transit(params, bus, |g| g.activate()),
        "goal.pause" => transit(params, bus, |g| g.pause()),
        "goal.resume" => transit(params, bus, |g| g.resume()),
        "goal.cancel" => transit(params, bus, |g| g.cancel()),
        "goal.complete" => {
            let evidence = params.get("evidence").and_then(Value::as_str).ok_or("missing evidence")?.to_string();
            transit(params, bus, |g| g.complete(&evidence))
        }
        "goal.record_turn" => {
            let tokens = params.get("tokens").and_then(Value::as_u64).unwrap_or(0);
            let reason = params.get("blocked_reason").and_then(Value::as_str).map(String::from);
            let terminal = params.get("terminal").and_then(Value::as_bool).unwrap_or(false);
            transit(params, bus, |g| g.record_turn(tokens, reason.as_deref(), terminal))
        }
        other => Err(format!("unknown goal method: {other}")),
    }
}

fn publish(bus: &kxen_app::core::event::EventBus, goal: &Goal) {
    bus.publish(Event::GoalUpdate { id: goal.id.clone(), status: status_str(goal.status) });
}

fn status_str(status: kxen_app::core::goal::GoalStatus) -> &'static str {
    match status {
        kxen_app::core::goal::GoalStatus::Draft => "draft",
        kxen_app::core::goal::GoalStatus::Queued => "queued",
        kxen_app::core::goal::GoalStatus::Active => "active",
        kxen_app::core::goal::GoalStatus::Paused => "paused",
        kxen_app::core::goal::GoalStatus::Blocked => "blocked",
        kxen_app::core::goal::GoalStatus::BudgetLimited => "budget_limited",
        kxen_app::core::goal::GoalStatus::Complete => "complete",
        kxen_app::core::goal::GoalStatus::Canceled => "canceled",
    }
}

fn load(id: &str) -> Result<Goal, String> {
    Goal::load(&dir(), id).map_err(|e| e.to_string())
}

fn transit(
    params: Value,
    bus: &kxen_app::core::event::EventBus,
    f: impl FnOnce(&mut Goal) -> Result<(), kxen_app::core::goal::GoalError>,
) -> Result<Value, String> {
    let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
    let mut goal = load(id)?;
    f(&mut goal).map_err(|e| e.to_string())?;
    goal.save(&dir()).map_err(|e| e.to_string())?;
    publish(bus, &goal);
    Ok(to_json(&goal))
}
