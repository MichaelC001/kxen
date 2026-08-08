use crate::AppState;
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) const METHODS: &[&str] = &[
    "knowledge.list",
    "knowledge.add",
    "knowledge.remove",
    "knowledge.set_enabled",
    "knowledge.move",
    "knowledge.injection_preview",
    "knowledge.consolidation_blocked",
    "knowledge.consolidation_acknowledge_unknown",
];

pub(super) async fn handle(method: &str, params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    match method {
        "knowledge.list" => {
            let dir = kxen_core::core::shared::read(&state.active_workspace).clone();
            serde_json::to_value(kxen_core::knowledge::list(&dir)).map_err(|error| error.to_string())
        }
        "knowledge.add" => {
            let scope = kxen_core::knowledge::Scope::parse(params.get("scope").and_then(Value::as_str).unwrap_or("personal"))?;
            let slug = params.get("slug").and_then(Value::as_str);
            let kind = params.get("type").and_then(Value::as_str).unwrap_or("note");
            let description = params.get("description").and_then(Value::as_str).ok_or("missing description")?;
            let content = params.get("content").and_then(Value::as_str).ok_or("missing content")?;
            let dir = kxen_core::core::shared::read(&state.active_workspace).clone();
            let path = kxen_core::knowledge::add(scope, &dir, slug, kind, description, content)?;
            Ok(json!({ "path": path }))
        }
        "knowledge.remove" => {
            let scope = kxen_core::knowledge::Scope::parse(params.get("scope").and_then(Value::as_str).ok_or("missing scope")?)?;
            let slug = params.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
            let dir = kxen_core::core::shared::read(&state.active_workspace).clone();
            kxen_core::knowledge::remove(scope, &dir, slug)?;
            Ok(json!({ "removed": true }))
        }
        "knowledge.set_enabled" => {
            let scope = kxen_core::knowledge::Scope::parse(params.get("scope").and_then(Value::as_str).ok_or("missing scope")?)?;
            let slug = params.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
            let enabled = params.get("enabled").and_then(Value::as_bool).ok_or("missing enabled")?;
            let dir = kxen_core::core::shared::read(&state.active_workspace).clone();
            kxen_core::knowledge::set_enabled(scope, &dir, slug, enabled)?;
            Ok(json!({ "scope": scope.as_str(), "slug": slug, "enabled": enabled }))
        }
        "knowledge.move" => {
            let scope = kxen_core::knowledge::Scope::parse(params.get("scope").and_then(Value::as_str).ok_or("missing scope")?)?;
            let slug = params.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
            let to = kxen_core::knowledge::Scope::parse(params.get("to").and_then(Value::as_str).ok_or("missing to")?)?;
            let dir = kxen_core::core::shared::read(&state.active_workspace).clone();
            let path = kxen_core::knowledge::move_entry(scope, &dir, slug, to)?;
            Ok(json!({ "path": path }))
        }
        "knowledge.injection_preview" => {
            let session_id = params.get("session_id").and_then(Value::as_str);
            let dir = match session_id {
                Some(session_id) => state.runtime_for_session(session_id)?.root().to_path_buf(),
                None => kxen_core::core::shared::read(&state.active_workspace).clone(),
            };
            let involved = session_id
                .and_then(|session_id| kxen_core::core::shared::lock(&state.session_involved).get(session_id).cloned())
                .unwrap_or_default();
            Ok(json!({ "block": kxen_core::knowledge::render(&dir, &involved) }))
        }
        "knowledge.consolidation_blocked" => {
            serde_json::to_value(kxen_core::knowledge::consolidate::blocked_attempts()?).map_err(|error| error.to_string())
        }
        "knowledge.consolidation_acknowledge_unknown" => {
            if params.get("confirm_unknown").and_then(Value::as_bool) != Some(true) {
                return Err("confirm_unknown must be true".into());
            }
            let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let result = kxen_core::knowledge::consolidate::acknowledge_unknown(session_id, &state.session_tokens).await?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        }
        _ => Err("unknown knowledge method".into()),
    }
}
