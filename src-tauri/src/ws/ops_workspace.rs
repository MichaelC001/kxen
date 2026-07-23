//! workspace 域 RPC：中心看板卡片数据（仿 ops_provider 分文件模式）。

use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::AppState;

pub(super) const METHODS: &[&str] = &["workspaces.overview"];

pub(super) async fn handle(method: &str, _params: &Value, app: &AppHandle) -> Result<Value, String> {
    match method {
        "workspaces.overview" => {
            let state = app.state::<Arc<AppState>>();
            let sessions = kxen_app::core::session::list(&kxen_app::core::paths::sessions_dir());
            let running: std::collections::HashSet<String> =
                kxen_app::core::shared::lock(&state.active_runs).keys().cloned().collect();
            let workspaces = kxen_app::core::workspace::list(&kxen_app::core::paths::data_dir());
            Ok(json!(kxen_app::core::workspace::overview(workspaces, &sessions, &running)))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}
