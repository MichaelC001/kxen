//! 状态栏与设置。

use serde_json::{Value, json};
use std::sync::Arc;

use crate::AppState;

pub(super) fn statusline_report(session_id: &str, state: &Arc<AppState>) -> Value {
    let items = kxen_app::core::shared::lock(&state.statusline_items).clone();

    // git 分支（5s 缓存）
    let git_branch = {
        let mut cache = kxen_app::core::shared::lock(&state.git_cache);
        if cache.0.elapsed() > std::time::Duration::from_secs(5) {
            let branch = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&*state.active_workspace.read().expect("workspace"))
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            *cache = (std::time::Instant::now(), branch);
        }
        cache.1.clone()
    };

    // statusline 跟当前 session 的 goal 焦点（P2-08）：多会话并发各看各的，空 id 回落全局
    let focus = kxen_app::core::goal::Goal::focus_for(
        &kxen_app::core::paths::goals_dir(),
        if session_id.is_empty() { None } else { Some(session_id) },
    );
    let tasks_running = state.registry.list().iter().filter(|t| matches!(t.status, kxen_app::tools::task::TaskStatus::Running)).count();
    let tokens = kxen_app::core::shared::lock(&state.session_tokens).get(session_id).copied().unwrap_or((0, 0));
    let last_input = kxen_app::core::shared::lock(&state.session_last_input).get(session_id).copied().unwrap_or(0);
    let model = super::session_ops::effective_session_model(if session_id.is_empty() { None } else { Some(session_id) }, state);
    // ctx 占用近似：最近一次 run 的 input / 模型上下文窗（catalog 实测值，非 200k 硬编码）
    let window = kxen_app::agent::compact::context_window(&model) as f64;
    let ctx_pct = ((last_input as f64 / window) * 100.0).min(100.0) as u32;

    json!({
        "items": items,
        "workdir": state.active_workspace.read().expect("workspace").to_string_lossy(),
        "git_branch": git_branch,
        "goal": focus.map(|g| json!({ "id": g.id, "status": format!("{:?}", g.status).to_lowercase() })),
        "tasks_running": tasks_running,
        "tokens": { "input": tokens.0, "output": tokens.1 },
        "ctx_pct": ctx_pct,
        "model": format!("{}/{}", model.provider, model.model),
    })
}

/// 非破坏写回：toml::Value 上改 roles[role]，保留文件其余内容；随后重建 MRM 热换 Arc。
pub(super) fn set_role(
    role: &str,
    provider: &str,
    model: &str,
    fallback: Option<&str>,
    account: Option<&str>,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    let path = kxen_app::core::paths::config_dir().join("config.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    // toml 1.x：Value::from_str 解析的是「值」不是文档，文档必须按 Table 解析
    let mut doc: toml::Table =
        if text.trim().is_empty() { toml::Table::new() } else { toml::from_str(&text).map_err(|e| format!("config.toml parse: {e}"))? };
    let roles = doc.entry(String::from("roles")).or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let roles_table = roles.as_table_mut().ok_or("roles is not a table")?;
    let binding = merge_binding(roles_table.get(role).and_then(toml::Value::as_table), provider, model, fallback, account);
    roles_table.insert(role.into(), toml::Value::Table(binding));

    std::fs::create_dir_all(kxen_app::core::paths::config_dir()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, toml::to_string(&doc).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;

    // 重建 MRM 热换
    let config = kxen_app::core::config::Config::load(&path, None).map_err(|e| e.to_string())?;
    *state.mrm.write().expect("mrm lock") = std::sync::Arc::new(kxen_app::llm::mrm::ModelResourceManager::new(config));
    Ok(json!({ "role": role, "provider": provider, "model": model }))
}

/// 合并新旧 binding（P0-4 数据不丢的主防线，双保险以后端为准：RPC 面向所有调用方，
/// 前端全量带字段只是其中之一）。旧实现整表重建，缺省参数直接抹掉旧值——切 provider 丢
/// fallback+account、改 model 丢降级链。约定：None = 未提及沿用旧值；Some("") = 显式清除
/// （前端选「无降级/账号轮转」、provider 变更清 account 走这里）；Some(v) = 覆盖。
fn merge_binding(
    old: Option<&toml::map::Map<String, toml::Value>>,
    provider: &str,
    model: &str,
    fallback: Option<&str>,
    account: Option<&str>,
) -> toml::map::Map<String, toml::Value> {
    fn field(old: Option<&toml::map::Map<String, toml::Value>>, key: &str, new: Option<&str>) -> Option<toml::Value> {
        match new {
            None => old.and_then(|t| t.get(key)).cloned(),
            Some("") => None,
            Some(v) => Some(toml::Value::String(v.into())),
        }
    }
    let mut binding = toml::map::Map::new();
    binding.insert("provider".into(), toml::Value::String(provider.into()));
    binding.insert("model".into(), toml::Value::String(model.into()));
    if let Some(f) = field(old, "fallback", fallback) {
        binding.insert("fallback".into(), f);
    }
    if let Some(a) = field(old, "account", account) {
        binding.insert("account".into(), a);
    }
    binding
}

#[cfg(test)]
mod tests {
    use super::merge_binding;

    fn old_binding() -> toml::map::Map<String, toml::Value> {
        let mut t = toml::map::Map::new();
        t.insert("provider".into(), toml::Value::String("anthropic".into()));
        t.insert("model".into(), toml::Value::String("claude-opus-4-1".into()));
        t.insert("fallback".into(), toml::Value::String("execution".into()));
        t.insert("account".into(), toml::Value::String("work".into()));
        t
    }

    fn get<'a>(b: &'a toml::map::Map<String, toml::Value>, key: &str) -> Option<&'a str> {
        b.get(key).and_then(toml::Value::as_str)
    }

    #[test]
    fn omitted_fields_fall_back_to_old_binding() {
        // P0-4 回归：切 provider / 改 model 缺省调用不再丢 fallback+account
        let old = old_binding();
        let b = merge_binding(Some(&old), "openai", "gpt-5.2", None, None);
        assert_eq!(get(&b, "provider"), Some("openai"));
        assert_eq!(get(&b, "model"), Some("gpt-5.2"));
        assert_eq!(get(&b, "fallback"), Some("execution"));
        assert_eq!(get(&b, "account"), Some("work"));
    }

    #[test]
    fn explicit_empty_string_clears_field() {
        // 清除语义：Some("") 删除字段（沿用旧值会清不掉，这是与 None 的关键区分）
        let old = old_binding();
        let b = merge_binding(Some(&old), "anthropic", "claude-opus-4-1", Some(""), Some(""));
        assert!(!b.contains_key("fallback"));
        assert!(!b.contains_key("account"));
    }

    #[test]
    fn overwrite_wins_and_fresh_role_has_no_defaults() {
        let old = old_binding();
        let b = merge_binding(Some(&old), "anthropic", "m", Some("review"), Some("team"));
        assert_eq!(get(&b, "fallback"), Some("review"));
        assert_eq!(get(&b, "account"), Some("team"));
        // 新建角色：无旧值可沿用，缺省即缺省
        let b = merge_binding(None, "anthropic", "m", None, None);
        assert!(!b.contains_key("fallback"));
        assert!(!b.contains_key("account"));
    }
}
