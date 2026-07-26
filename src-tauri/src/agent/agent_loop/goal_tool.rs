//! goal 工具：目标生命周期管理（list/create/get/activate/pause/resume/cancel/complete）。
//! 状态迁移成功后 publish GoalUpdate（此前只落盘不发布，Dock 面板对 /write-goal 主流程零刷新）。

use serde_json::Value;

/// complete 的逐条验证评审（score-based）：模型自带 model+store，judge 由调用方择型注入。
pub struct GoalJudge {
    pub model: crate::llm::ModelRef,
    pub store: crate::auth::credential::AuthStore,
}

pub async fn execute_goal_tool(
    args: &Value,
    session_id: Option<&str>,
    bus: Option<&crate::core::event::EventBus>,
    judge: Option<&GoalJudge>,
) -> Result<String, String> {
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
            Ok(if goals.is_empty() { "no goals".into() } else { goals.iter().map(&show).collect::<Vec<_>>().join("\n---\n") })
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
            publish(bus, &goal);
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
                    // score-based 逐条验证：全过才允许 complete；评审调用失败按可重试错误返回，不降级放行
                    if let Some(j) = judge {
                        let scores = crate::agent::goal_verify::score_completion(
                            &j.model,
                            &j.store,
                            &goal.contract.objective,
                            &goal.contract.completion_criteria,
                            evidence,
                        )
                        .await?;
                        let failed: Vec<_> = scores.iter().filter(|s| !s.pass).collect();
                        if !failed.is_empty() {
                            let detail = failed.iter().map(|s| format!("- {}: {}", s.criterion, s.reason)).collect::<Vec<_>>().join("\n");
                            return Err(format!(
                                "completion verification failed ({} criterion/criteria unmet):\n{detail}\n\
                                 Provide evidence that actually satisfies every criterion, or adjust the goal contract.",
                                failed.len()
                            ));
                        }
                    }
                    goal.complete(evidence).map_err(|e| e.to_string())?;
                }
                unknown => return Err(format!("unknown goal action: {unknown}")),
            }
            goal.save(&dir).map_err(|e| e.to_string())?;
            // get 只读无状态迁移，不发事件
            if other != "get" {
                publish(bus, &goal);
            }
            Ok(show(&goal))
        }
    }
}

/// 与 goal_rpc.rs 同一收口：GoalUpdate payload 形态一致（id + snake_case 状态串），Dock goal 面板据此刷新。
fn publish(bus: Option<&crate::core::event::EventBus>, goal: &crate::core::goal::Goal) {
    if let Some(bus) = bus {
        bus.publish(crate::core::event::Event::GoalUpdate { id: goal.id.clone(), status: goal.status.as_str() });
    }
}
