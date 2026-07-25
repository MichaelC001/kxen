mod cron_dispatch;
mod doctor;
mod goal_rpc;
mod ws;

use kxen_app::llm::ModelRef;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub struct AppState {
    /// 共享句柄（Arc 内层不变）：TeamManager SpawnDeps 持同一把锁，凭证探测/刷新后操作点可见
    pub auth_store: Arc<Mutex<kxen_app::auth::credential::AuthStore>>,
    /// ws 服务端口（serve 成功后写入，ws_port command 用）
    ws_port: Mutex<u16>,
    /// ws 握手 token（启动时 /dev/urandom 生成，ws_port command 一并发给前端）
    pub ws_token: String,
    model: Mutex<ModelRef>,
    pub bus: kxen_app::core::event::EventBus,
    pub registry: std::sync::Arc<kxen_app::tools::task::TaskRegistry>,
    /// 角色路由可热更新（设置页改角色 -> 重建换 Arc）；与 SpawnDeps 共享同一 RwLock 句柄
    pub mrm: std::sync::Arc<std::sync::RwLock<std::sync::Arc<kxen_app::llm::mrm::ModelResourceManager>>>,
    pub extras: std::sync::Arc<kxen_app::agent::agent_loop::SessionExtrasRegistry>,
    /// Ask 档审批 broker（exec 高危命令的用户决定路由）
    pub approvals: std::sync::Arc<kxen_app::agent::approval::ApprovalBroker>,
    /// MCP server 管理器（mcp__server__tool 工具桥）
    pub mcp: std::sync::Arc<kxen_app::mcp::McpManager>,
    /// LSP 诊断（per-workspace 懒启动 rust-analyzer；切换工作区时重建）
    pub lsp: std::sync::RwLock<std::sync::Arc<kxen_app::lsp::LspManager>>,
    pub hooks: std::sync::Arc<kxen_app::tools::hooks::HookRunner>,
    pub team: std::sync::Arc<kxen_app::agent::team::TeamManager>,
    pub agents: std::sync::Arc<kxen_app::agent::activity::AgentRegistry>,
    /// session_id -> 进行中 run 的取消令牌（session.abort 用；run 结束自行移除）
    pub active_runs: std::sync::Mutex<std::collections::HashMap<String, kxen_app::agent::cancel::CancelToken>>,
    /// session 排队消息（落盘持久化：崩溃重启可恢复续跑；run 结束按序接续，防并发 run 交叉写历史）
    pub pending_messages: std::sync::Arc<kxen_app::core::pending_queue::PendingQueues>,
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
    /// session_id -> agent 改动快照（改动面板数据源；run 间共享，随 app 存活）
    pub session_snapshots: std::sync::Mutex<std::collections::HashMap<String, kxen_app::tools::snapshot::SnapshotStore>>,
    /// session_id -> 最近一轮 run 的 involved 文件（injection_preview 的真实 glob 命中数据源）
    pub session_involved: std::sync::Mutex<std::collections::HashMap<String, Vec<std::path::PathBuf>>>,
    /// 通知环形缓冲（teammate/cron/系统事件，顶栏通知中心数据源，50 条）
    pub notifications: std::sync::Mutex<std::collections::VecDeque<kxen_app::core::notifications::Notice>>,
    /// 前台聚焦会话（OS 通知只发非前台会话的完成事件）
    pub foreground_session: std::sync::RwLock<String>,
    /// 原生对话框附件授权清单（选择即授权；context 边界守卫与 read_attachment 的唯一放行依据）
    pub picked_files: kxen_app::core::attachment::PickedFiles,
}

impl AppState {
    #[allow(dead_code)]
    fn new() -> Self {
        let path = kxen_app::core::paths::auth_file();
        // 共享句柄：与 TeamManager SpawnDeps 同一把锁，后台探测写入的凭证两边即时可见；
        // 登记回写后 run 内刷新（ctx.store 是克隆快照）也即时收敛到各克隆点（auth::shared_store）
        let store = Arc::new(Mutex::new(kxen_app::auth::credential::read_auth_file(&path)));
        kxen_app::auth::shared_store::register_shared_store(&store);
        let config =
            kxen_app::core::config::Config::load(&kxen_app::core::paths::config_dir().join("config.toml"), None).unwrap_or_default();
        let statusline_items = config.statusline.items.clone();
        let registry = std::sync::Arc::new(kxen_app::tools::task::TaskRegistry::new());
        let extras = std::sync::Arc::new(kxen_app::agent::agent_loop::SessionExtrasRegistry::default());
        let hooks = std::sync::Arc::new(kxen_app::tools::hooks::HookRunner::from_config(&config));
        let mrm = std::sync::Arc::new(std::sync::RwLock::new(std::sync::Arc::new(kxen_app::llm::mrm::ModelResourceManager::new(config))));
        let workdir: std::sync::Arc<std::path::Path> =
            std::sync::Arc::from(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")));
        let bus = kxen_app::core::event::EventBus::default();
        let agents = std::sync::Arc::new(kxen_app::agent::activity::AgentRegistry::default());
        let approvals = std::sync::Arc::new(kxen_app::agent::approval::production_broker(bus.clone()));
        let mcp = kxen_app::mcp::McpManager::new();
        let lsp = std::sync::RwLock::new(kxen_app::lsp::LspManager::new(workdir.to_path_buf()));
        // P0-2：team relay 与 AppState 共享同一队列实例（teammate 报告入队 = 用户消息同路续跑）
        let pending_messages =
            std::sync::Arc::new(kxen_app::core::pending_queue::PendingQueues::new(kxen_app::core::paths::sessions_dir()));
        let team = kxen_app::agent::team::TeamManager::new(
            kxen_app::core::paths::data_dir().join("teams"),
            kxen_app::agent::team::SpawnDeps {
                registry: registry.clone(),
                fallback_workdir: workdir.clone(),
                store: store.clone(),
                mrm: mrm.clone(),
                hooks: Some(hooks.clone()),
                extras: extras.clone(),
                agents: agents.clone(),
                approvals: Some(approvals.clone()),
                mcp: Some(mcp.clone()),
                // team 自持 LSP 池：AppState.lsp 随 workspace switch 重建，member 绑 session 目录不漂移
                lsp: std::sync::Arc::new(kxen_app::agent::team::LspPool::default()),
            },
            bus.clone(),
            kxen_app::core::paths::sessions_dir(),
            Some(pending_messages.clone()),
        );
        Self {
            auth_store: store,
            ws_port: Mutex::new(0),
            ws_token: ws::gen_ws_token(),
            model: Mutex::new(ModelRef::new("xai", "grok-build-0.1")),
            bus,
            registry,
            extras,
            approvals,
            mcp,
            lsp,
            hooks,
            team,
            agents,
            active_runs: std::sync::Mutex::new(std::collections::HashMap::new()),
            pending_messages,
            run_streams: std::sync::Mutex::new(std::collections::HashMap::new()),
            mrm,
            session_tokens: std::sync::Mutex::new(kxen_app::core::usage::load()),
            session_last_input: std::sync::Mutex::new(std::collections::HashMap::new()),
            statusline_items: std::sync::Mutex::new(statusline_items),
            git_cache: std::sync::Mutex::new((std::time::Instant::now() - std::time::Duration::from_secs(60), String::new())),
            active_workspace: std::sync::RwLock::new(workdir.to_path_buf()),
            session_snapshots: std::sync::Mutex::new(std::collections::HashMap::new()),
            session_involved: std::sync::Mutex::new(std::collections::HashMap::new()),
            notifications: std::sync::Mutex::new(kxen_app::core::notifications::load()),
            foreground_session: std::sync::RwLock::new(String::new()),
            picked_files: kxen_app::core::attachment::PickedFiles::default(),
            workdir,
        }
    }

    /// 主会话 extras 取口：按 session 隔离，跨 send_message 存续。
    pub fn extras_for(&self, session_id: &str) -> std::sync::Arc<kxen_app::agent::agent_loop::SessionExtras> {
        self.extras.extras_for(session_id)
    }

    /// 会话销毁时清 extras（删除会话路径尚未接线，暂无人调）。
    #[allow(dead_code)]
    pub fn drop_extras(&self, session_id: &str) {
        self.extras.drop_extras(session_id);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();

    tauri::async_runtime::block_on(async {
        let app = tauri::Builder::default()
            .plugin(tauri_plugin_websocket::init())
            .plugin(tauri_plugin_notification::init())
            .plugin(tauri_plugin_dialog::init())
            .invoke_handler(tauri::generate_handler![ws_port])
            .manage(Arc::new(AppState::new()))
            .setup(|app| {
                // macOS 原生编辑菜单：WKWebView 的 Cmd+C/V/X/A/Z 由菜单栏分发，无菜单则编辑快捷键全灭
                use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
                let edit = Submenu::with_items(
                    app,
                    "编辑",
                    true,
                    &[
                        &PredefinedMenuItem::undo(app, None)?,
                        &PredefinedMenuItem::redo(app, None)?,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::cut(app, None)?,
                        &PredefinedMenuItem::copy(app, None)?,
                        &PredefinedMenuItem::paste(app, None)?,
                        &PredefinedMenuItem::select_all(app, None)?,
                    ],
                )?;
                let app_menu = Submenu::with_items(
                    app,
                    "kxen",
                    true,
                    &[
                        &PredefinedMenuItem::hide(app, None)?,
                        &PredefinedMenuItem::hide_others(app, None)?,
                        &PredefinedMenuItem::quit(app, None)?,
                    ],
                )?;
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
                // 崩溃前排队的消息恢复续跑；teammate -> lead 与 background late 通知在无活跃 run 时的续跑触发
                ws::pending::restore_queues(app.handle().clone());
                ws::pending::wire_team_kick(app.handle());
                ws::pending::wire_background_kick(app.handle());
                // 通知落盘：bus 订阅一条，Notification 事件进环形缓冲（通知中心数据源）
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use kxen_app::core::event::{RecvVerdict, recv_verdict};
                    let mut rx = handle.state::<Arc<AppState>>().bus.subscribe();
                    // Lagged 跳过继续收（静默退出 = 通知中心永久停更），Closed（app 退出）才停
                    loop {
                        let event = match recv_verdict(rx.recv().await) {
                            RecvVerdict::Event(e) => e,
                            RecvVerdict::Skip => continue,
                            RecvVerdict::Stop => break,
                        };
                        // 非前台会话的 run 完成：OS 桌面通知（前台会话用户在看，不打扰）
                        if let kxen_app::core::event::Event::LlmDelta(payload) = &event {
                            if payload.get("kind").and_then(|k| k.as_str()) == Some("done") {
                                let sid = payload.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
                                let state = handle.state::<Arc<AppState>>();
                                let fg = state.foreground_session.read().expect("foreground").clone();
                                if !sid.is_empty() && sid != fg {
                                    use tauri_plugin_notification::NotificationExt;
                                    let title = kxen_app::core::session::load_meta(&kxen_app::core::paths::sessions_dir(), sid)
                                        .map(|m| m.title)
                                        .unwrap_or_else(|_| sid.to_string());
                                    let _ = handle.notification().builder().title("kxen 会话完成").body(&title).show();
                                }
                            }
                        }
                        if let kxen_app::core::event::Event::Notification { text, session_id } = event {
                            // notification hook（全部 Notification 事件的单一收口点；Ask 档走审批）
                            let state = handle.state::<Arc<AppState>>();
                            let hooks = state.hooks.clone();
                            // broker/bus 克隆进任务（借用无法跨 spawn 的 'static 边界）
                            let broker = state.approvals.clone();
                            let bus = state.bus.clone();
                            let (text2, sid) = (text.clone(), session_id.clone());
                            tauri::async_runtime::spawn(async move {
                                let appr = kxen_app::tools::exec::ApprovalCtx::new(Some(broker.as_ref()), Some(&bus), None, None);
                                let payload = &serde_json::json!({ "text": text2, "session_id": sid });
                                if let Err(e) = hooks.run_named_with_approval("notification", &text2, payload, appr.as_ref()).await {
                                    tracing::warn!(error = %e, "notification hook failed");
                                }
                            });
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis() as u64)
                                .unwrap_or(0);
                            let state = handle.state::<Arc<AppState>>();
                            let mut buf = state.notifications.lock().expect("notifications");
                            kxen_app::core::notifications::push(&mut buf, now, text, session_id);
                            kxen_app::core::notifications::persist(&buf);
                        }
                    }
                });
                // cron tick：15s 一轮，到期任务注入会话起 run（进程内调度，随 app 存活）
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut ticks = 0u32;
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                        ticks += 1;
                        // 后台记忆 consolidation：120 tick（30min）一轮，best-effort
                        if ticks % 120 == 0 {
                            let state = handle.state::<Arc<AppState>>();
                            let model = state.model.lock().map(|m| m.clone()).unwrap_or_default();
                            let store = state.auth_store.lock().map(|s| s.clone()).unwrap_or_default();
                            let written = kxen_app::knowledge::consolidate::run_once(&model, &store).await;
                            if written > 0 {
                                tracing::info!(written, "memory consolidation distilled");
                            }
                        }
                        let now =
                            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
                        // 同批已派发的 session：首个 spawn 后 token 尚未注册进 active_runs，靠本集合判重
                        let mut dispatched: std::collections::HashSet<String> = std::collections::HashSet::new();
                        for job in kxen_app::core::schedule::drain_due(now) {
                            let state = handle.state::<Arc<AppState>>();
                            let has_active = kxen_app::core::shared::lock(&state.active_runs).contains_key(&job.session_id);
                            let has_queued = state.pending_messages.has_queued(&job.session_id);
                            let text = format!("[cron {}] {}", job.id, job.prompt);
                            match cron_dispatch::cron_dispatch(has_active, has_queued, dispatched.contains(&job.session_id)) {
                                cron_dispatch::CronDispatch::Spawn => {
                                    dispatched.insert(job.session_id.clone());
                                    let stream_id = ws::protocol::stream_id("run");
                                    kxen_app::core::shared::lock(&state.run_streams).insert(stream_id.clone(), job.session_id.clone());
                                    tokio::spawn(ws::llm_task::run_llm(stream_id, job.session_id, text, vec![], vec![], handle.clone()));
                                }
                                // 并发 run 会交叉写 JSONL 历史：投入队列由 run 结束续跑消化
                                cron_dispatch::CronDispatch::Enqueue => {
                                    let n = state.pending_messages.enqueue(&job.session_id, text, vec![], vec![]);
                                    let note = format!("cron 触发时会话运行中，已排队（第 {n} 条）");
                                    state.bus.publish(kxen_app::core::event::Event::notify(note, Some(job.session_id.clone())));
                                }
                            }
                        }
                    }
                });
                // MCP servers：信任门 + 双 scope 加载后台启动（server 冷启动可至 60s，绝不阻塞启动路径）
                {
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let state = handle.state::<Arc<AppState>>();
                        let workdir = state.active_workspace.read().expect("workspace").clone();
                        kxen_app::mcp::reload_for_workspace(&workdir, &state.mcp).await;
                    });
                }
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

/// 前端拿 ws 端口 + 握手 token（替代 window.eval 注入：页面重载后注入丢失的竞态根治）。
#[tauri::command]
fn ws_port(state: tauri::State<'_, Arc<AppState>>) -> serde_json::Value {
    let port = *state.ws_port.lock().expect("ws_port");
    serde_json::json!({ "port": port, "token": state.ws_token })
}
