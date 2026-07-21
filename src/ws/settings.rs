//! 状态栏与设置。

use serde_json::{json, Value};
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

    let focus = kxen_app::core::goal::Goal::focus(&kxen_app::core::paths::goals_dir());
    let tasks_running = state.registry.list().iter().filter(|t| matches!(t.status, kxen_app::tools::task::TaskStatus::Running)).count();
    let tokens = kxen_app::core::shared::lock(&state.session_tokens).get(session_id).copied().unwrap_or((0, 0));
    let last_input = kxen_app::core::shared::lock(&state.session_last_input).get(session_id).copied().unwrap_or(0);
    let model = state.model.lock().map(|m| m.clone()).unwrap_or_default();
    // ctx 占用近似：最近一次 run 的 input / 200k 窗（Claude Code 只按 input 算的惯例）
    let ctx_pct = ((last_input as f64 / 200_000.0) * 100.0).min(100.0) as u32;

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
pub(super) fn set_role(role: &str, provider: &str, model: &str, fallback: Option<&str>, state: &Arc<AppState>) -> Result<Value, String> {
    let path = kxen_app::core::paths::config_dir().join("config.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: toml::Value = if text.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        text.parse().map_err(|e| format!("config.toml parse: {e}"))?
    };
    let table = doc.as_table_mut().ok_or("config.toml root is not a table")?;
    let roles = table.entry(String::from("roles")).or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let roles_table = roles.as_table_mut().ok_or("roles is not a table")?;
    let mut binding = toml::map::Map::new();
    binding.insert("provider".into(), toml::Value::String(provider.into()));
    binding.insert("model".into(), toml::Value::String(model.into()));
    if let Some(f) = fallback {
        binding.insert("fallback".into(), toml::Value::String(f.into()));
    }
    roles_table.insert(role.into(), toml::Value::Table(binding));

    std::fs::create_dir_all(kxen_app::core::paths::config_dir()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, doc.to_string()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;

    // 重建 MRM 热换
    let config = kxen_app::core::config::Config::load(&path, None).map_err(|e| e.to_string())?;
    *state.mrm.write().expect("mrm lock") = std::sync::Arc::new(kxen_app::llm::mrm::ModelResourceManager::new(config));
    Ok(json!({ "role": role, "provider": provider, "model": model }))
}
