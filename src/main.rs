mod doctor;
mod goal_rpc;
mod ws;

use kxen_app::llm::ModelRef;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub struct AppState {
    auth_store: Mutex<kxen_app::auth::credential::AuthStore>,
    /// ws 服务端口（serve 成功后写入，ws_port command 用）
    ws_port: Mutex<u16>,
    model: Mutex<ModelRef>,
    pub bus: kxen_app::core::event::EventBus,
    pub registry: std::sync::Arc<kxen_app::tools::task::TaskRegistry>,
    /// 角色路由可热更新（设置页改角色 -> 重建换 Arc）
    pub mrm: std::sync::RwLock<std::sync::Arc<kxen_app::llm::mrm::ModelResourceManager>>,
    pub extras: std::sync::Arc<kxen_app::agent::agent_loop::SessionExtras>,
    pub hooks: std::sync::Arc<kxen_app::tools::hooks::HookRunner>,
    pub team: std::sync::Arc<kxen_app::agent::team::TeamManager>,
    pub agents: std::sync::Arc<kxen_app::agent::activity::AgentRegistry>,
    /// session_id -> 进行中 run 的取消令牌（session.abort 用；run 结束自行移除）
    pub active_runs: std::sync::Mutex<std::collections::HashMap<String, kxen_app::agent::cancel::CancelToken>>,
    /// stream_id -> session_id（rpc.cancelStream 路由用）
    pub run_streams: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// session_id -> (input, output) tokens 累计（状态栏用量段）
    pub session_tokens: std::sync::Mutex<std::collections::HashMap<String, (u64, u64)>>,
    /// session_id -> 最近一次 run 的 input tokens（ctx 占用近似值，进度条数据源）
    pub session_last_input: std::sync::Mutex<std::collections::HashMap<String, u64>>,
    /// 状态栏显隐段（启动时从 config 读；设置页改后重建）
    pub statusline_items: std::sync::Mutex<Vec<String>>,
    /// git 分支 5s 缓存（状态栏 git 段，防每帧 spawn）
    pub git_cache: std::sync::Mutex<(std::time::Instant, String)>,
    pub workdir: std::sync::Arc<std::path::Path>,
}

impl AppState {
    #[allow(dead_code)]
    fn new() -> Self {
        let path = kxen_app::core::paths::auth_file();
        let mut store = kxen_app::auth::credential::read_auth_file(&path);
        let outcomes = kxen_app::auth::probe_all(&mut store);
        let _ = kxen_app::auth::credential::write_auth_file(&path, &store);
        for (provider, outcome, _) in &outcomes {
            tracing::info!(provider, ?outcome, "credential probe");
        }
        let config = kxen_app::core::config::Config::load(
            &kxen_app::core::paths::config_dir().join("config.toml"),
            None,
        )
        .unwrap_or_default();
        let statusline_items = config.statusline.items.clone();
        let registry = std::sync::Arc::new(kxen_app::tools::task::TaskRegistry::new());
        let extras = std::sync::Arc::new(kxen_app::agent::agent_loop::SessionExtras::default());
        let hooks = std::sync::Arc::new(kxen_app::tools::hooks::HookRunner::from_config(&config));
        let mrm = std::sync::Arc::new(kxen_app::llm::mrm::ModelResourceManager::new(config));
        let workdir: std::sync::Arc<std::path::Path> =
            std::sync::Arc::from(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")));
        let bus = kxen_app::core::event::EventBus::default();
        let agents = std::sync::Arc::new(kxen_app::agent::activity::AgentRegistry::default());
        let team = kxen_app::agent::team::TeamManager::new(
            kxen_app::core::paths::data_dir().join("teams"),
            kxen_app::agent::team::SpawnDeps {
                registry: registry.clone(),
                workdir: workdir.clone(),
                store: store.clone(),
                mrm: mrm.clone(),
                hooks: Some(hooks.clone()),
                extras: extras.clone(),
                agents: agents.clone(),
            },
            bus.clone(),
        );
        Self {
            auth_store: Mutex::new(store),
            ws_port: Mutex::new(0),
            model: Mutex::new(ModelRef::new("xai", "grok-build-0.1")),
            bus,
            registry,
            extras,
            hooks,
            team,
            agents,
            active_runs: std::sync::Mutex::new(std::collections::HashMap::new()),
            run_streams: std::sync::Mutex::new(std::collections::HashMap::new()),
            mrm: std::sync::RwLock::new(mrm),
            session_tokens: std::sync::Mutex::new(std::collections::HashMap::new()),
            session_last_input: std::sync::Mutex::new(std::collections::HashMap::new()),
            statusline_items: std::sync::Mutex::new(statusline_items),
            git_cache: std::sync::Mutex::new((std::time::Instant::now() - std::time::Duration::from_secs(60), String::new())),
            workdir,
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::async_runtime::block_on(async {
        let app = tauri::Builder::default()
            .plugin(tauri_plugin_websocket::init())
            .invoke_handler(tauri::generate_handler![ws_port])
            .manage(Arc::new(AppState::new()))
            .setup(|app| {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    match ws::serve(handle.clone()).await {
                        Ok(port) => {
                            tracing::info!(port, "ws server listening");
                            if let Some(state) = handle.try_state::<Arc<AppState>>() {
                                *state.ws_port.lock().expect("ws_port") = port;
                            }
                        }
                        Err(e) => tracing::error!(error = %e, "ws server failed"),
                    }
                });
                Ok(())
            })
            .build(tauri::generate_context!())
            .expect("error while building kxen");

        app.run(|_, _| {});
    });
}

fn main() {
    run();
}


/// 前端拿 ws 端口（替代 window.eval 注入：页面重载后注入丢失的竞态根治）。
#[tauri::command]
fn ws_port(state: tauri::State<'_, Arc<AppState>>) -> u16 {
    *state.ws_port.lock().expect("ws_port")
}
