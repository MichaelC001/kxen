//! goal RPC 方法（goal.{list,create,activate,pause,resume,complete,cancel,get}）。

use kxen_core::goal::{Goal, GoalBudget, GoalContract};
use kxen_core::paths;
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

pub fn call(method: &str, params: Value) -> Result<Value, String> {
    match method {
        "goal.list" => {
            let goals = Goal::list(&dir());
            Ok(json!(goals.iter().map(to_json).collect::<Vec<_>>()))
        }
        "goal.focus" => Ok(Goal::focus(&dir()).map(|g| to_json(&g)).unwrap_or(Value::Null)),
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
            let id = format!("goal_{}_{:06x}", now_ms(), std::process::id());
            let goal = Goal::create(contract, id).map_err(|e| e.to_string())?;
            goal.save(&dir()).map_err(|e| e.to_string())?;
            Ok(to_json(&goal))
        }
        "goal.get" => {
            let goal = load(params.get("id").and_then(Value::as_str).ok_or("missing id")?)?;
            Ok(to_json(&goal))
        }
        "goal.activate" => transit(params, |g| g.activate()),
        "goal.pause" => transit(params, |g| g.pause()),
        "goal.resume" => transit(params, |g| g.resume()),
        "goal.cancel" => transit(params, |g| g.cancel()),
        "goal.complete" => {
            let evidence = params.get("evidence").and_then(Value::as_str).ok_or("missing evidence")?.to_string();
            transit(params, |g| g.complete(&evidence))
        }
        "goal.record_turn" => {
            let tokens = params.get("tokens").and_then(Value::as_u64).unwrap_or(0);
            let reason = params.get("blocked_reason").and_then(Value::as_str).map(String::from);
            let terminal = params.get("terminal").and_then(Value::as_bool).unwrap_or(false);
            transit(params, |g| g.record_turn(tokens, reason.as_deref(), terminal))
        }
        other => Err(format!("unknown goal method: {other}")),
    }
}

fn load(id: &str) -> Result<Goal, String> {
    Goal::load(&dir(), id).map_err(|e| e.to_string())
}

fn transit(params: Value, f: impl FnOnce(&mut Goal) -> Result<(), kxen_core::goal::GoalError>) -> Result<Value, String> {
    let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
    let mut goal = load(id)?;
    f(&mut goal).map_err(|e| e.to_string())?;
    goal.save(&dir()).map_err(|e| e.to_string())?;
    Ok(to_json(&goal))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
