mod os_notify;
mod tray;

use kxen_core::AppState;
use kxen_core::web::{WebServer, WebServerHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Manager;

/// 用户配置加载失败时回退默认，不阻塞启动。
fn load_user_config() -> kxen_core::core::config::Config {
    let path = kxen_core::core::paths::config_dir().join("config.toml");
    kxen_core::core::config::Config::load(&path, None).unwrap_or_else(|error| {
        tracing::warn!(%error, "user config load failed, falling back to defaults");
        kxen_core::core::config::Config::default()
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();

    tauri::async_runtime::block_on(async {
        let state = match AppState::new() {
            Ok(state) => Arc::new(state),
            Err(e) => {
                tracing::error!(error = %e, "app state initialization failed");
                return;
            }
        };
        let web_handle: Arc<Mutex<Option<WebServerHandle>>> = Arc::new(Mutex::new(None));
        let web_handle_setup = web_handle.clone();
        let config = load_user_config();
        let close_to_tray = Arc::new(AtomicBool::new(config.tray.close_to_tray));
        let close_to_tray_window = close_to_tray.clone();
        let config_setup = config;
        let app = tauri::Builder::default()
            .plugin(tauri_plugin_notification::init())
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init())
            .plugin(tauri_plugin_opener::init())
            .on_window_event(move |window, event| {
                // 关窗转隐藏驻留托盘；真正退出走 tray「退出 Kxen」/ Cmd+Q（app.exit 不受阻）
                if let tauri::WindowEvent::CloseRequested { api, .. } = event
                    && window.label() == "main"
                    && close_to_tray_window.load(Ordering::Relaxed)
                {
                    let _ = window.hide();
                    api.prevent_close();
                }
            })
            .invoke_handler(tauri::generate_handler![ws_port])
            .manage(state)
            .setup(move |app| {
                // 窗口代码建：titleBarStyle/hiddenTitle 仅 macOS，配置 JSON 无法按平台分支
                let window_builder = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                    .title("Kxen")
                    .inner_size(1280.0, 800.0)
                    .min_inner_size(1280.0, 800.0)
                    .resizable(true)
                    .fullscreen(false);
                // shadow 而非 mut 重绑定：非 macOS 无此臂，mut 会触发 unused_mut
                #[cfg(target_os = "macos")]
                let window_builder = window_builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
                window_builder.build()?;
                // macOS：WKWebView 编辑快捷键由菜单栏分发，无菜单则全灭；其余平台 set_menu 会多出系统菜单栏
                #[cfg(target_os = "macos")]
                {
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
                }
                let state = app.state::<Arc<AppState>>().inner().clone();
                *kxen_core::core::shared::write(&state.notify) = os_notify::desktop_target(app.handle());
                // 端口占用回退随机，实际端口写回 state.ws_port
                let bind: std::net::IpAddr = config_setup.web.bind.parse().unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
                let web_enabled = config_setup.web.enabled;
                let started = WebServer::start((bind, config_setup.web.port), state.clone(), web_enabled, Vec::new()).or_else(|error| {
                    tracing::warn!(error = %error, port = config_setup.web.port, "preferred web port unavailable, falling back to random");
                    WebServer::start((bind, 0), state.clone(), web_enabled, Vec::new())
                });
                match started {
                    Ok(handle) => {
                        let port = handle.port();
                        *kxen_core::core::shared::lock(&state.ws_port) = port;
                        if let Ok(mut slot) = web_handle_setup.lock() {
                            *slot = Some(handle);
                        }
                        tracing::info!(port, "web server listening");
                    }
                    Err(error) => tracing::error!(error = %error, "web server failed"),
                }
                match tray::setup(
                    app.handle(),
                    state.clone(),
                    web_handle_setup,
                    bind.to_string(),
                    web_enabled,
                    tray::DefaultOpen::parse(&config_setup.tray.default_open),
                    close_to_tray,
                ) {
                    Ok(guard) => {
                        app.manage(guard);
                    }
                    Err(error) => tracing::error!(error = %error, "tray setup failed"),
                }
                // 崩溃前排队恢复；teammate/background 在无活跃 run 时的续跑触发
                kxen_core::ws::pending::restore_queues(state.clone());
                kxen_core::ws::pending::wire_team_kick(&state);
                kxen_core::ws::pending::wire_background_kick(&state);
                {
                    let state = state.clone();
                    kxen_core::notify_sink::spawn_with(state.clone(), move |event| {
                        if let kxen_core::core::event::Event::LlmDelta(payload) = event {
                            let should_notify = {
                                let foreground = kxen_core::core::shared::read(&state.foreground_session);
                                os_notify::should_notify_done(payload, &foreground)
                            };
                            if should_notify {
                                let sid = payload.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
                                let title = kxen_core::core::session::load_meta(&kxen_core::core::paths::sessions_dir(), sid)
                                    .map(|m| m.title)
                                    .unwrap_or_else(|_| sid.to_string());
                                os_notify::notify_session_done(kxen_core::core::shared::read(&state.notify).clone(), sid, &title);
                            }
                        }
                    });
                }
                // Provider 慢请求不得阻塞 cron / Knowledge consolidation
                kxen_core::background_jobs::spawn(state.clone());
                // MCP/runtime 冷启动可至 60s，后台加载绝不阻塞启动路径
                {
                    let state = state.clone();
                    tauri::async_runtime::spawn(async move {
                        let workdir = kxen_core::core::shared::read(&state.active_workspace).clone();
                        if let Err(e) = state.workspace_runtimes.ready(&workdir).await {
                            tracing::warn!(error = %e, "initial workspace runtime failed");
                        }
                    });
                }
                // keychain 读取可被 ACL 弹窗无限阻塞，凭证探测必须走后台
                tauri::async_runtime::spawn(async move {
                    let baseline = kxen_core::core::shared::lock(&state.auth_store).clone();
                    let probed = tokio::task::spawn_blocking(move || {
                        let mut store = (*baseline).clone();
                        let outcomes = kxen_core::auth::probe_all(&mut store, false);
                        (baseline, store, outcomes)
                    })
                    .await;
                    if let Ok((baseline, store, outcomes)) = probed {
                        for (provider, outcome, _) in &outcomes {
                            tracing::info!(provider, ?outcome, "credential probe");
                        }
                        let mut current = kxen_core::core::shared::lock(&state.auth_store);
                        match kxen_core::auth::credential::update_auth_file(&kxen_core::core::paths::auth_file(), |disk| {
                            kxen_core::auth::probe::merge_probe_delta(&baseline, &store, disk);
                            Ok(())
                        }) {
                            Ok(persisted) => *current = std::sync::Arc::new(persisted),
                            Err(error) => tracing::error!(%error, "credential probe persistence failed"),
                        }
                    }
                });
                Ok(())
            })
            .build(tauri::generate_context!())
            .expect("error while building kxen");

        app.run(move |_, event| {
            if let tauri::RunEvent::Exit = event
                && let Ok(guard) = web_handle.lock()
                && let Some(handle) = guard.as_ref()
            {
                handle.shutdown();
            }
        });
    });
}

fn main() {
    run();
}

#[cfg(test)]
fn should_dispatch_schedule(sessions_dir: &std::path::Path, session_id: &str) -> Result<bool, String> {
    kxen_core::core::session_recovery::is_tombstoned(sessions_dir, session_id).map(|tombstoned| !tombstoned)
}

/// 前端拿 ws 端口 + 握手 token（替代 window.eval 注入，避免页面重载后丢失）。
#[tauri::command]
fn ws_port(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let port = *kxen_core::core::shared::lock(&state.ws_port);
    ws_endpoint(port, &state.ws_token)
}

fn ws_endpoint(port: u16, token: &str) -> Result<serde_json::Value, String> {
    if port == 0 {
        return Err("websocket server is not ready".into());
    }
    Ok(serde_json::json!({ "port": port, "token": token }))
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod workspace_tests;
