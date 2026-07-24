//! goal 工具：目标生命周期管理（list/create/get/activate/pause/resume/cancel/complete）。

use serde_json::Value;

pub async fn execute_goal_tool(args: &Value, session_id: Option<&str>) -> Result<String, String> {
    let action = args.get("action").and_then(Value::as_str).ok_or("missing action")?;
    let dir = crate::core::paths::goals_dir();
    let show = |g: &crate::core::goal::Goal| {
        format!(
            "goal {} [{}] {}\ncriteria: {}\nturns: {} tokens: {} blocks: {}{}",
            g.id,
            format!("{:?}", g.status).to_lowercase(),
            g.contract.objective,
            g.contract.completion_criteria,
            g.turns_used,
            g.tokens_used,
            g.consecutive_blocks,
            g.block_reason.as_deref().map(|r| format!("\nblocked: {r}")).unwrap_or_default()
        )
    };
    match action {
        "list" => {
            let goals = crate::core::goal::Goal::list(&dir);
            Ok(if goals.is_empty() { "no goals".into() } else { goals.iter().map(|g| show(g)).collect::<Vec<_>>().join("\n---\n") })
        }
        "create" => {
            let contract = crate::core::goal::GoalContract {
                objective: args.get("objective").and_then(Value::as_str).ok_or("missing objective")?.to_string(),
                completion_criteria: args
                    .get("completion_criteria")
                    .and_then(Value::as_str)
                    .ok_or("missing completion_criteria")?
                    .to_string(),
                constraints: args.get("constraints").and_then(Value::as_str).map(String::from),
                budget: crate::core::goal::GoalBudget {
                    tokens: args.pointer("/budget/tokens").and_then(Value::as_u64),
                    turns: args.pointer("/budget/turns").and_then(Value::as_u64).map(|n| n as u32),
                    wall_clock_ms: args.pointer("/budget/wall_clock_ms").and_then(Value::as_u64),
                },
            };
            let id = crate::core::ids::new_id("goal");
            let mut goal = crate::core::goal::Goal::create(contract, id).map_err(|e| e.to_string())?;
            goal.session_id = session_id.map(String::from);
            goal.save(&dir).map_err(|e| e.to_string())?;
            Ok(show(&goal))
        }
        other => {
            let id = args.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let mut goal = crate::core::goal::Goal::load(&dir, id).map_err(|e| e.to_string())?;
            match other {
                "get" => {}
                "activate" => goal.activate().map_err(|e| e.to_string())?,
                "pause" => goal.pause().map_err(|e| e.to_string())?,
                "resume" => goal.resume().map_err(|e| e.to_string())?,
                "cancel" => goal.cancel().map_err(|e| e.to_string())?,
                "complete" => {
                    let evidence = args.get("evidence").and_then(Value::as_str).ok_or("missing evidence")?;
                    goal.complete(evidence).map_err(|e| e.to_string())?;
                }
                unknown => return Err(format!("unknown goal action: {unknown}")),
            }
            goal.save(&dir).map_err(|e| e.to_string())?;
            Ok(show(&goal))
        }
    }
}
