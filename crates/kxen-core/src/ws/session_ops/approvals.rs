//! 审批类 RPC 下沉（350 行门禁）：pending 快照 / 应答(可建规) / 规则管理 / 审计历史。

use serde_json::{Value, json};

/// approval.respond RPC：remember="session"|"workspace" 时放行成功后建前缀规则（B1）；
/// 建规失败不撤回放行，以 rule_error 回传前端提示。
pub(in crate::ws) fn approval_respond(params: &Value, state: &crate::AppState) -> Result<Value, String> {
    let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
    let allow = params.get("allow").and_then(Value::as_bool).ok_or("missing allow")?;
    let remember = params
        .get("remember")
        .and_then(Value::as_str)
        .map(|raw| kxen_core::agent::approval_rules::RuleScope::parse(raw).ok_or_else(|| format!("invalid remember scope: {raw}")))
        .transpose()?;
    let outcome = state.approvals.respond_ext(id, allow, remember);
    let mut out = json!({ "resolved": outcome.delivered });
    if let Some(rule_id) = outcome.rule_id {
        out.as_object_mut().expect("respond outcome").insert("rule_id".into(), json!(rule_id));
    }
    if let Some(error) = outcome.rule_error {
        out.as_object_mut().expect("respond outcome").insert("rule_error".into(), json!(error));
    }
    Ok(out)
}

/// approval.pending RPC：带 session_id 返回该会话审批；省略时只返回全局审批。
/// 两个恢复面互斥，避免同一 approval 同时出现在 Layout 与 Session。
pub(in crate::ws) fn approval_pending(params: &Value, state: &crate::AppState) -> Result<Value, String> {
    let sid = params.get("session_id").and_then(Value::as_str);
    Ok(json!(state.approvals.list_pending(sid)))
}

/// approval.history RPC（B7 审计视图）：投影落盘的 Part::Approval + created_at，按时间倒序。
/// 有 session_id 只读该会话；省略时遍历全部会话（设置页全局视图）。limit 缺省 200，上限 1000。
pub(in crate::ws) fn approval_history(params: &Value) -> Result<Value, String> {
    approval_history_in(&kxen_core::core::paths::sessions_dir(), params)
}

pub(in crate::ws) fn approval_history_in(dir: &std::path::Path, params: &Value) -> Result<Value, String> {
    let sid = params.get("session_id").and_then(Value::as_str).filter(|id| !id.is_empty());
    let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(200).min(1000) as usize;
    let session_ids: Vec<String> = match sid {
        Some(id) => vec![id.to_string()],
        None => kxen_core::core::session::list(dir).into_iter().map(|s| s.id).collect(),
    };
    let mut rows = Vec::new();
    for id in &session_ids {
        for msg in kxen_core::core::session::load_messages(dir, id) {
            for part in &msg.parts {
                if let kxen_core::core::session::Part::Approval { command, reason, decision } = part {
                    rows.push(json!({
                        "session_id": id,
                        "created_at": msg.created_at,
                        "command": command,
                        "reason": reason,
                        "decision": decision,
                    }));
                }
            }
        }
    }
    rows.sort_by_key(|row| std::cmp::Reverse(row["created_at"].as_u64().unwrap_or(0)));
    rows.truncate(limit);
    Ok(json!(rows))
}

/// approval_rules.list RPC：session_id 可解析时含该会话规则；workspace 取其会话目录，否则 active_workspace。
pub(in crate::ws) fn approval_rules_list(params: &Value, state: &crate::AppState) -> Result<Value, String> {
    let sid = params.get("session_id").and_then(Value::as_str);
    let workspace = approval_rules_workspace(sid, state);
    Ok(json!(state.approvals.list_rules(sid, &workspace)))
}

/// approval_rules.revoke RPC：按 id 摘除 session 内存规则或 workspace 落盘规则。
pub(in crate::ws) fn approval_rules_revoke(params: &Value, state: &crate::AppState) -> Result<Value, String> {
    let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
    let sid = params.get("session_id").and_then(Value::as_str);
    let workspace = approval_rules_workspace(sid, state);
    Ok(json!({ "revoked": state.approvals.revoke_rule(id, &workspace)? }))
}

/// approval_rules.list/revoke 的 workspace 解析：session_id 可解析时取其会话目录，
/// 否则回落 active_workspace（设置页无会话上下文的全局视图）。
fn approval_rules_workspace(session_id: Option<&str>, state: &crate::AppState) -> std::path::PathBuf {
    session_id
        .filter(|id| !id.is_empty())
        .and_then(|id| kxen_core::core::session::load_meta(&kxen_core::core::paths::sessions_dir(), id).ok())
        .map(|meta| std::path::PathBuf::from(meta.directory))
        .unwrap_or_else(|| kxen_core::core::shared::read(&state.active_workspace).clone())
}
