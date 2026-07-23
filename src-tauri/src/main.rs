mod doctor;
mod goal_rpc;
mod ws;

use kxen_app::llm::ModelRef;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub struct AppState {
    pub auth_store: Mutex<kxen_app::auth::credential::AuthStore>,
    /// ws 服务端口（serve 成功后写入，ws_port command 用）
    ws_port: Mutex<u16>,
    model: Mutex<ModelRef>,
    pub bus: kxen_app::core::event::EventBus,
    pub registry: std::sync::Arc<kxen_app::tools::task::TaskRegistry>,
    /// 角色路由可热更新（设置页改角色 -> 重建换 Arc）；与 SpawnDeps 共享同一 RwLock 句柄
    pub mrm: std::sync::Arc<std::sync::RwLock<std::sync::Arc<kxen_app::llm::mrm::ModelResourceManager>>>,
    pub extras: std::sync::Arc<kxen_app::agent::agent_loop::SessionExtras>,
    pub hooks: std::sync::Arc<kxen_app::tools::hooks::HookRunner>,
    pub team: std::sync::Arc<kxen_app::agent::team::TeamManager>,
    pub agents: std::sync::Arc<kxen_app::agent::activity::AgentRegistry>,
    /// session_id -> 进行中 run 的取消令牌（session.abort 用；run 结束自行移除）
    pub active_runs: std::sync::Mutex<std::collections::HashMap<String, kxen_app::agent::cancel::CancelToken>>,
    /// session_id -> 排队消息（run 进行中收到的发送；run 结束按序接续，防并发 run 交叉写历史）
    pub pending_messages: std::sync::Mutex<
        std::collections::HashMap<
            String,
            std::collections::VecDeque<(String, Vec<kxen_app::agent::context::ContextItem>, Vec<kxen_app::llm::types::ImagePart>)>,
        >,
    >,
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
    /// 当前活跃 workspace（多项目目录，可切换；初始 = workdir）
    pub active_workspace: std::sync::RwLock<std::path::PathBuf>,
    /// 通知环形缓冲（teammate/cron/系统事件，顶栏通知中心数据源，50 条）
    pub notifications: std::sync::Mutex<std::collections::VecDeque<(u64, String)>>,
}

impl AppState {
    #[allow(dead_code)]
    fn new() -> Self {
        let path = kxen_app::core::paths::auth_file();
        let store = kxen_app::auth::credential::read_auth_file(&path);
        let config = kxen_app::core::config::Config::load(
            &kxen_app::core::paths::config_dir().join("config.toml"),
            None,
        )
        .unwrap_or_default();
        let statusline_items = config.statusline.items.clone();
        let registry = std::sync::Arc::new(kxen_app::tools::task::TaskRegistry::new());
        let extras = std::sync::Arc::new(kxen_app::agent::agent_loop::SessionExtras::default());
        let hooks = std::sync::Arc::new(kxen_app::tools::hooks::HookRunner::from_config(&config));
        let mrm = std::sync::Arc::new(std::sync::RwLock::new(std::sync::Arc::new(
            kxen_app::llm::mrm::ModelResourceManager::new(config),
        )));
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
            pending_messages: std::sync::Mutex::new(std::collections::HashMap::new()),
            run_streams: std::sync::Mutex::new(std::collections::HashMap::new()),
            mrm,
            session_tokens: std::sync::Mutex::new(std::collections::HashMap::new()),
            session_last_input: std::sync::Mutex::new(std::collections::HashMap::new()),
            statusline_items: std::sync::Mutex::new(statusline_items),
            git_cache: std::sync::Mutex::new((std::time::Instant::now() - std::time::Duration::from_secs(60), String::new())),
            active_workspace: std::sync::RwLock::new(workdir.to_path_buf()),
            notifications: std::sync::Mutex::new(std::collections::VecDeque::new()),
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
                // macOS 原生编辑菜单：WKWebView 的 Cmd+C/V/X/A/Z 由菜单栏分发，无菜单则编辑快捷键全灭
                use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
                let edit = Submenu::with_items(app, "编辑", true, &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ])?;
                let app_menu = Submenu::with_items(app, "kxen", true, &[
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::quit(app, None)?,
                ])?;
                app.set_menu(Menu::with_items(app, &[&app_menu, &edit])?)?;
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
                // 通知落盘：bus 订阅一条，Notification 事件进环形缓冲（通知中心数据源）
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut rx = handle.state::<Arc<AppState>>().bus.subscribe();
                    while let Ok(event) = rx.recv().await {
                        if let kxen_app::core::event::Event::Notification(text) = event {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis() as u64)
                                .unwrap_or(0);
                            let state = handle.state::<Arc<AppState>>();
                            let mut buf = state.notifications.lock().expect("notifications");
                            buf.push_front((now, text));
                            buf.truncate(50);
                        }
                    }
                });
                // cron tick：15s 一轮，到期任务注入会话起 run（进程内调度，随 app 存活）
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        for job in kxen_app::core::schedule::drain_due(now) {
                            let stream_id = ws::protocol::stream_id("run");
                            let state = handle.state::<Arc<AppState>>();
                            kxen_app::core::shared::lock(&state.run_streams).insert(stream_id.clone(), job.session_id.clone());
                            let text = format!("[cron {}] {}", job.id, job.prompt);
                            tokio::spawn(ws::llm_task::run_llm(stream_id, job.session_id, text, vec![], vec![], handle.clone()));
                        }
                    }
                });
                // 凭证探测走后台：keychain 读取可被 ACL 弹窗无限阻塞，绝不能卡启动路径
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let probed = tokio::task::spawn_blocking(|| {
                        let path = kxen_app::core::paths::auth_file();
                        let mut store = kxen_app::auth::credential::read_auth_file(&path);
                        let outcomes = kxen_app::auth::probe_all(&mut store, false);
                        let _ = kxen_app::auth::credential::write_auth_file(&path, &store);
                        (store, outcomes)
                    })
                    .await;
                    if let Ok((store, outcomes)) = probed {
                        for (provider, outcome, _) in &outcomes {
                            tracing::info!(provider, ?outcome, "credential probe");
                        }
                        if let Some(state) = handle.try_state::<Arc<AppState>>() {
                            *state.auth_store.lock().expect("auth_store") = store;
                        }
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
