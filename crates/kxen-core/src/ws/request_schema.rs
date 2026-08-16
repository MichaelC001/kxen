//! 业务 RPC 的 JSON 参数契约。结构错误在进入 handler 前统一映射为 -32602。

use serde_json::Value;

use super::protocol::{CallError, value_kind};

#[path = "request_schema/bot_fields.rs"]
mod bot_fields;
#[path = "request_schema/methods.rs"]
mod methods;

#[derive(Clone, Copy)]
pub(super) enum Kind {
    String,
    Bool,
    Array,
    StringArray,
    U64,
    Number,
    Object,
}

#[derive(Debug)]
pub(super) enum ValidatedParams {
    SendMessage(super::session_ops::SendMessageParams),
    Other(Value),
}

pub(super) fn validate_rpc(method: &str, params: Value) -> Result<ValidatedParams, CallError> {
    if !methods::METHODS.contains(&method) {
        return Err(CallError::method_not_found(method));
    }
    for &(field, kind) in required_fields(method) {
        let Some(value) = params.get(field) else {
            return Err(CallError::invalid_params(method, field, kind.expected(), "missing"));
        };
        if !kind.valid(value, true) {
            return Err(CallError::invalid_params(method, field, kind.expected(), value_kind(value)));
        }
    }
    for &(field, kind) in optional_fields(method) {
        let Some(value) = params.get(field) else { continue };
        if !kind.valid(value, false) {
            return Err(CallError::invalid_params(method, field, kind.expected(), value_kind(value)));
        }
    }
    reject_unknown_bot_fields(method, &params)?;
    validate_values(method, &params)?;
    if method == "send_message" {
        serde_json::from_value(params)
            .map(ValidatedParams::SendMessage)
            .map_err(|_| CallError::invalid_params(method, "$", "valid send_message object", "object"))
    } else {
        Ok(ValidatedParams::Other(params))
    }
}

impl Kind {
    fn valid(self, value: &Value, required: bool) -> bool {
        if !required && value.is_null() {
            return true;
        }
        match self {
            Self::String => value.as_str().is_some_and(|value| !required || !value.is_empty()),
            Self::Bool => value.is_boolean(),
            Self::Array => value.is_array(),
            Self::StringArray => {
                value.as_array().is_some_and(|values| values.iter().all(|value| value.as_str().is_some_and(|item| !item.is_empty())))
            }
            Self::U64 => value.as_u64().is_some(),
            Self::Number => value.as_f64().is_some_and(|number| number.is_finite() && number >= 0.0),
            Self::Object => value.is_object(),
        }
    }

    fn expected(self) -> &'static str {
        match self {
            Self::String => "non-empty string",
            Self::Bool => "boolean",
            Self::Array => "array",
            Self::StringArray => "string array",
            Self::U64 => "non-negative integer",
            Self::Number => "non-negative number",
            Self::Object => "object",
        }
    }
}

fn required_fields(method: &str) -> &'static [(&'static str, Kind)] {
    if let Some(fields) = bot_fields::required(method) {
        return fields;
    }
    use Kind::{Bool as B, Object as O, String as S, StringArray as SA};
    match method {
        "task.kill"
        | "session.activate"
        | "session.messages"
        | "session.delete"
        | "session.pending_list"
        | "session.pending_clear"
        | "schedule.remove" => &[("id", S)],
        "task.restart" => &[("id", S), ("session_id", S)],
        "recovery.clear" | "recovery.inspect" | "recovery.repair" => &[("session_id", S)],
        "workspace.add" | "workspace.switch" => &[("path", S)],
        "session.fork" | "session.rewind" => &[("session_id", S), ("message_id", S)],
        "session.export" | "session.abort" | "session.rewind_undo" => &[("session_id", S)],
        "session.context_stats" => &[("session_id", S)],
        "session.update_meta" | "session.set_model" => &[("id", S)],
        "send_message" => &[("session_id", S)],
        "approval.respond" => &[("id", S), ("allow", B)],
        "approval_rules.revoke" => &[("id", S)],
        "team.message" => &[("session_id", S), ("name", S), ("text", S)],
        "agents.stop" | "agents.dismiss" => &[("session_id", S), ("name", S)],
        "agents.transcript" => &[("name", S)],
        "config.set_role" => &[("role", S), ("provider", S), ("model", S)],
        "composer.suggest.local" => &[("draft", S)],
        "composer.suggest.remote" => &[("draft", S), ("session_id", S), ("request_id", S), ("candidate_ids", SA)],
        "composer.suggest.cancel" => &[("session_id", S)],
        "fs.resolve_name" => &[("name", S)],
        "fs.allow_path" | "fs.read_attachment" => &[("session_id", S), ("path", S)],
        "coding_rules.set" => &[("enabled", B)],
        "knowledge.add" => &[("description", S), ("content", S)],
        "knowledge.remove" => &[("scope", S), ("slug", S)],
        "knowledge.set_enabled" => &[("scope", S), ("slug", S), ("enabled", B)],
        "knowledge.move" => &[("scope", S), ("slug", S), ("to", S)],
        "knowledge.consolidation_acknowledge_unknown" => &[("session_id", S), ("confirm_unknown", B)],
        "schedule.add" => &[("cron", S), ("prompt", S), ("session_id", S)],
        "schedule.set_enabled" => &[("id", S), ("enabled", B)],
        "voice.set_provider_key" => &[("provider", S), ("key", S)],
        "voice.set_engine" => &[("engine", S)],
        "config.set_send_policy" => &[("policy", S)],
        "config.set_experimental" => &[("key", S), ("enabled", B)],
        "config.set_composer_suggestions" => &[("key", S), ("enabled", B)],
        "agent.test_dispatch" => &[("role", S)],
        "provider.verify" | "provider.models" => &[("provider", S)],
        "provider.import_account" => &[("provider", S), ("account", S), ("access", S)],
        "provider.remove_account" | "provider.set_region" => &[("provider", S), ("account", S)],
        "provider.add_custom" => &[("name", S), ("base_url", S), ("models", SA), ("api_key", S)],
        "provider.remove_custom" | "mcp.restart" | "mcp.auth" | "worktree.create" | "worktree.remove" | "worktree.apply" => &[("name", S)],
        "worktree.status" | "diff.file" => &[("path", S)],
        "diff.agent_status" => &[("session_id", S)],
        "diff.agent_file" => &[("session_id", S), ("path", S)],
        "goal.create" => &[("objective", S), ("completion_criteria", S)],
        "goal.activate" | "goal.pause" | "goal.resume" | "goal.cancel" | "goal.adjust" => &[("id", S)],
        "kanban.boards" | "kanban.board_create" => &[("workspace", S)],
        "kanban.snapshot" => &[("workspace", S), ("board", S)],
        "kanban.card_create" => &[("workspace", S), ("board", S), ("title", S)],
        "kanban.card_move" => &[("workspace", S), ("board", S), ("card_id", S), ("outcome", S)],
        "kanban.card_comment" | "kanban.run_start" => &[("workspace", S), ("board", S), ("card_id", S)],
        "kanban.policy_set" => &[("workspace", S), ("board", S), ("policy", O)],
        _ => &[],
    }
}

fn optional_fields(method: &str) -> &'static [(&'static str, Kind)] {
    if let Some(fields) = bot_fields::optional(method) {
        return fields;
    }
    use Kind::{Array as A, Bool as B, Number as N, Object as O, String as S, StringArray as SA, U64 as U};
    match method {
        "current_model"
        | "task.list"
        | "goal.focus"
        | "knowledge.injection_preview"
        | "approval.pending"
        | "agents.list"
        | "agents.transcript"
        | "statusline"
        | "voice.stop" => &[("session_id", S)],
        "task.kill" => &[("session_id", S)],
        "session.create" => &[("directory", S)],
        "session.delete" => &[("distill", B)],
        "session.update_meta" => &[("title", S), ("pinned", B), ("sort_order", U)],
        "session.set_model" => &[("provider", S), ("model", S)],
        "session.fork" => &[("position", S), ("kind", S)],
        "session.rewind" => &[("confirm", B)],
        "session.export" => &[("path", S)],
        "approval.respond" => &[("remember", S)],
        "approval.history" => &[("session_id", S), ("limit", U)],
        "approval_rules.list" | "approval_rules.revoke" => &[("session_id", S)],
        "send_message" => &[("text", S), ("context", A), ("images", A)],
        "config.set_role" => &[("fallback", S), ("account", S)],
        "fs.complete" => &[("query", S), ("limit", U)],
        "composer.suggest.local" => &[("session_id", S), ("selected_paths", SA), ("limit", U)],
        "composer.suggest.remote" => &[("selected_paths", SA), ("limit", U)],
        "composer.suggest.cancel" => &[("request_id", S)],
        "knowledge.add" => &[("scope", S), ("slug", S), ("type", S)],
        "schedule.add" => &[("once", B)],
        "voice.set_engine" => &[("fallback", SA), ("locale", S)],
        "voice.start" => &[("locale", S), ("engine", S), ("session_id", S)],
        "config.set_limits" => &[
            ("provider", S),
            ("daily_token_budget", U),
            ("input_usd_per_million", N),
            ("output_usd_per_million", N),
            ("daily_cost_budget_usd", N),
            ("circuit_failure_threshold", U),
            ("circuit_cooldown_seconds", U),
        ],
        "config.set_embedding" => &[("provider", S), ("model", S), ("base_url", S)],
        "provider.verify" => &[("account", S), ("model", S), ("access", S), ("kind", S), ("refresh", S), ("expires", U), ("region", S)],
        "provider.models" => &[("account", S)],
        "provider.import_account" => &[("kind", S), ("refresh", S), ("expires", U), ("region", S), ("account_id", S)],
        "provider.set_region" => &[("region", S)],
        "provider.add_custom" => &[("protocol", S), ("capabilities", SA), ("query_params", O)],
        "provider.probe_models" => &[("protocol", S), ("query_params", O)],
        "worktree.remove" => &[("delete_branch", B), ("confirmed", B)],
        "worktree.status" | "diff.status" | "diff.file" => &[("session_id", S)],
        "goal.create" => &[("constraints", S), ("session_id", S), ("budget", O)],
        "kanban.board_create" => &[("columns", A)],
        "kanban.card_create" => &[("body", S), ("column_id", S)],
        "kanban.card_comment" => &[("author", S)],
        _ => &[],
    }
}

fn validate_values(method: &str, params: &Value) -> Result<(), CallError> {
    let invalid = |field: &str, expected: &str| {
        CallError::invalid_params(method, field, expected, params.get(field).map(value_kind).unwrap_or("missing"))
    };
    match method {
        "bot.group.create" if params.get("bot_ids").and_then(Value::as_array).is_none_or(|bot_ids| !(2..=6).contains(&bot_ids.len())) => {
            Err(invalid("bot_ids", "array containing 2 to 6 Bot ids"))
        }
        "config.set_send_policy" if !matches!(params.get("policy").and_then(Value::as_str), Some("queue" | "interrupt")) => {
            Err(invalid("policy", "queue or interrupt"))
        }
        "config.set_experimental"
            if !matches!(
                params.get("key").and_then(Value::as_str),
                Some("automatic_knowledge_distillation" | "browser_automation" | "remote_mcp")
            ) =>
        {
            Err(invalid("key", "known experimental setting"))
        }
        "config.set_composer_suggestions" if !matches!(params.get("key").and_then(Value::as_str), Some("enabled" | "semantic" | "llm")) => {
            Err(invalid("key", "enabled, semantic, or llm"))
        }
        "session.set_model"
            if params.get("provider").and_then(Value::as_str).is_some() != params.get("model").and_then(Value::as_str).is_some() =>
        {
            Err(invalid("provider/model", "both present or both omitted"))
        }
        "session.fork"
            if !matches!(params.get("position").and_then(Value::as_str), None | Some("before" | "after"))
                || !matches!(params.get("kind").and_then(Value::as_str), None | Some("manual" | "edit" | "rerun")) =>
        {
            Err(invalid("position/kind", "before or after; manual, edit, or rerun"))
        }
        "provider.add_custom" if params.get("models").and_then(Value::as_array).is_none_or(Vec::is_empty) => {
            Err(invalid("models", "non-empty string array"))
        }
        "knowledge.consolidation_acknowledge_unknown" if params.get("confirm_unknown").and_then(Value::as_bool) != Some(true) => {
            Err(invalid("confirm_unknown", "true"))
        }
        _ => validate_nested(method, params, invalid),
    }
}

fn reject_unknown_bot_fields(method: &str, params: &Value) -> Result<(), CallError> {
    if !method.starts_with("bot.") {
        return Ok(());
    }
    let object = params.as_object().ok_or_else(|| CallError::invalid_params(method, "$", "object", value_kind(params)))?;
    if let Some(field) = object.keys().find(|field| {
        !required_fields(method).iter().any(|(known, _)| known == field) && !optional_fields(method).iter().any(|(known, _)| known == field)
    }) {
        return Err(CallError::invalid_params(method, field, "declared Bot RPC field", "unknown field"));
    }
    Ok(())
}

fn validate_nested(method: &str, params: &Value, invalid: impl Fn(&str, &str) -> CallError) -> Result<(), CallError> {
    if method.starts_with("knowledge.") {
        for field in ["scope", "to"] {
            if params.get(field).and_then(Value::as_str).is_some_and(|scope| kxen_core::knowledge::Scope::parse(scope).is_err()) {
                return Err(invalid(field, "project or personal"));
            }
        }
    }
    if method == "goal.create"
        && let Some(budget) = params.get("budget").and_then(Value::as_object)
    {
        for field in ["tokens", "turns", "wall_clock_ms"] {
            if budget.get(field).is_some_and(|value| !value.is_null() && value.as_u64().is_none()) {
                return Err(invalid("budget", "object with non-negative integer limits"));
            }
        }
        if budget.get("turns").and_then(Value::as_u64).is_some_and(|turns| turns > u32::MAX.into()) {
            return Err(invalid("budget.turns", "32-bit non-negative integer"));
        }
    }
    if method == "config.set_limits" {
        let scoped = [
            "input_usd_per_million",
            "output_usd_per_million",
            "daily_cost_budget_usd",
            "circuit_failure_threshold",
            "circuit_cooldown_seconds",
        ];
        if scoped.iter().any(|field| params.get(*field).is_some()) && params.get("provider").and_then(Value::as_str).is_none() {
            return Err(invalid("provider", "provider id for provider-scoped limits"));
        }
    }
    if method == "config.set_embedding"
        && !matches!(params.get("provider").and_then(Value::as_str), None | Some("" | "openai" | "openrouter" | "ollama"))
    {
        return Err(invalid("provider", "empty, openai, openrouter, or ollama"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "request_schema_tests.rs"]
mod tests;
