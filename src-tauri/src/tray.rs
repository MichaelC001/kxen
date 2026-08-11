//! 系统托盘：浏览器访问、默认打开、close-to-tray。
//! 菜单 setup 建一次后只改 text/enabled/checked，绝不 rebuild
//!（muda #173 macOS 展开期崩溃；Linux 不可替换；Windows 整树替换行为不一致）。
//! Linux libappindicator 不发 click，左键动作仅 macOS/Windows 注册。

mod logic;

pub use logic::DefaultOpen;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use kxen_core::AppState;
use kxen_core::web::WebServerHandle;
use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
#[cfg(not(target_os = "linux"))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

mod ids {
    pub const OPEN_WINDOW: &str = "tray:open-window";
    pub const OPEN_BROWSER: &str = "tray:open-browser";
    pub const COPY_URL: &str = "tray:copy-url";
    pub const WEB_ACCESS: &str = "tray:web-access";
    pub const DEFAULT_WINDOW: &str = "tray:default-window";
    pub const DEFAULT_BROWSER: &str = "tray:default-browser";
    pub const CLOSE_TO_TRAY: &str = "tray:close-to-tray";
    pub const CHECK_UPDATE: &str = "tray:check-update";
    pub const QUIT: &str = "tray:quit";
}

/// TrayIcon drop 即注销，由 app.manage 持有到进程退出。
pub struct TrayGuard {
    _tray: tauri::tray::TrayIcon,
}

struct Items {
    open_browser: MenuItem<Wry>,
    copy_url: MenuItem<Wry>,
    web_access: CheckMenuItem<Wry>,
    default_window: CheckMenuItem<Wry>,
    default_browser: CheckMenuItem<Wry>,
    close_item: CheckMenuItem<Wry>,
}

struct Shared {
    state: Arc<AppState>,
    web: Arc<Mutex<Option<WebServerHandle>>>,
    bind_host: String,
    close_to_tray: Arc<AtomicBool>,
    default_open: Arc<Mutex<DefaultOpen>>,
    items: Items,
}

#[allow(clippy::too_many_arguments)]
pub fn setup(
    app: &AppHandle,
    state: Arc<AppState>,
    web: Arc<Mutex<Option<WebServerHandle>>>,
    bind_host: String,
    web_enabled: bool,
    default_open: DefaultOpen,
    close_to_tray: Arc<AtomicBool>,
) -> tauri::Result<TrayGuard> {
    let port = *kxen_core::core::shared::lock(&state.ws_port);
    let url_available = logic::access_url(&bind_host, port, &state.ws_token).is_some();
    let browser_enabled = logic::browser_actions_enabled(web_enabled, url_available);

    let open_window = MenuItemBuilder::with_id(ids::OPEN_WINDOW, "打开 Kxen").build(app)?;
    let open_browser = MenuItemBuilder::with_id(ids::OPEN_BROWSER, "在浏览器中打开").enabled(browser_enabled).build(app)?;
    let copy_url = MenuItemBuilder::with_id(ids::COPY_URL, "复制访问链接").enabled(browser_enabled).build(app)?;
    let web_access =
        CheckMenuItemBuilder::with_id(ids::WEB_ACCESS, logic::web_access_label(&bind_host, port)).checked(web_enabled).build(app)?;
    let default_window =
        CheckMenuItemBuilder::with_id(ids::DEFAULT_WINDOW, "窗口").checked(default_open == DefaultOpen::Window).build(app)?;
    let default_browser =
        CheckMenuItemBuilder::with_id(ids::DEFAULT_BROWSER, "浏览器").checked(default_open == DefaultOpen::Browser).build(app)?;
    let default_submenu = SubmenuBuilder::new(app, "默认打开方式").items(&[&default_window, &default_browser]).build()?;
    let close_item = CheckMenuItemBuilder::with_id(ids::CLOSE_TO_TRAY, "关闭时最小化到托盘")
        .checked(close_to_tray.load(Ordering::Relaxed))
        .build(app)?;
    let check_update = MenuItemBuilder::with_id(ids::CHECK_UPDATE, "检查更新…").build(app)?;
    let quit = MenuItemBuilder::with_id(ids::QUIT, "退出 Kxen").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[
            &open_window,
            &open_browser,
            &copy_url,
            &PredefinedMenuItem::separator(app)?,
            &web_access,
            &PredefinedMenuItem::separator(app)?,
            &default_submenu,
            &close_item,
            &PredefinedMenuItem::separator(app)?,
            &check_update,
            &quit,
        ])
        .build()?;

    let shared = Arc::new(Shared {
        state,
        web,
        bind_host,
        close_to_tray,
        default_open: Arc::new(Mutex::new(default_open)),
        items: Items { open_browser, copy_url, web_access, default_window, default_browser, close_item },
    });

    let builder = TrayIconBuilder::new().menu(&menu).tooltip("Kxen").show_menu_on_left_click(false).icon(tray_icon()?);
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);
    #[cfg(not(target_os = "linux"))]
    let builder = {
        let shared = shared.clone();
        builder.on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                let action = shared.default_open.lock().map(|guard| *guard).unwrap_or(DefaultOpen::Window);
                match action {
                    DefaultOpen::Window => show_main_window(tray.app_handle()),
                    DefaultOpen::Browser => open_in_browser(tray.app_handle(), &shared),
                }
            }
        })
    };
    let shared_menu = shared;
    let tray = builder.on_menu_event(move |app, event| handle_menu_event(app, &shared_menu, event.id().as_ref())).build(app)?;
    Ok(TrayGuard { _tray: tray })
}

fn handle_menu_event(app: &AppHandle, shared: &Shared, id: &str) {
    match id {
        ids::OPEN_WINDOW => show_main_window(app),
        ids::OPEN_BROWSER => open_in_browser(app, shared),
        ids::COPY_URL => copy_access_url(shared),
        ids::WEB_ACCESS => toggle_web_access(shared),
        ids::DEFAULT_WINDOW => set_default_open(shared, DefaultOpen::Window),
        ids::DEFAULT_BROWSER => set_default_open(shared, DefaultOpen::Browser),
        ids::CLOSE_TO_TRAY => toggle_close_to_tray(shared),
        ids::CHECK_UPDATE => check_update(app),
        ids::QUIT => app.exit(0),
        _ => {}
    }
}

fn show_main_window(app: &AppHandle) {
    match app.get_webview_window("main") {
        Some(window) => {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        None => tracing::warn!("main window unavailable for tray open action"),
    }
}

fn access_url(shared: &Shared) -> Option<String> {
    let port = *kxen_core::core::shared::lock(&shared.state.ws_port);
    logic::access_url(&shared.bind_host, port, &shared.state.ws_token)
}

fn open_in_browser(app: &AppHandle, shared: &Shared) {
    use tauri_plugin_opener::OpenerExt;
    let Some(url) = access_url(shared) else {
        tracing::warn!("tray open in browser: web server is not running");
        return;
    };
    if let Err(error) = app.opener().open_url(&url, None::<&str>) {
        tracing::error!(%error, "tray open in browser failed");
    }
}

fn copy_access_url(shared: &Shared) {
    let Some(url) = access_url(shared) else {
        tracing::warn!("tray copy access url: web server is not running");
        return;
    };
    let result = arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(url));
    if let Err(error) = result {
        tracing::error!(%error, "tray copy access url failed");
    }
}

/// muda 点击后已自动翻转 checked，直接读新值即可。
fn toggle_web_access(shared: &Shared) {
    let enabled = shared.items.web_access.is_checked().unwrap_or(false);
    if let Ok(guard) = shared.web.lock()
        && let Some(handle) = guard.as_ref()
    {
        handle.set_static_enabled(enabled);
    }
    if let Err(error) = logic::persist_user_config(|doc| logic::set_bool(doc, "web", "enabled", enabled)) {
        tracing::error!(%error, enabled, "persist web.enabled failed");
    }
    let actions = logic::browser_actions_enabled(enabled, access_url(shared).is_some());
    let _ = shared.items.open_browser.set_enabled(actions);
    let _ = shared.items.copy_url.set_enabled(actions);
}

fn set_default_open(shared: &Shared, action: DefaultOpen) {
    if let Ok(mut guard) = shared.default_open.lock() {
        *guard = action;
    }
    // muda 只翻转被点项，另一互斥项须手动归位
    let _ = shared.items.default_window.set_checked(action == DefaultOpen::Window);
    let _ = shared.items.default_browser.set_checked(action == DefaultOpen::Browser);
    if let Err(error) = logic::persist_user_config(|doc| logic::set_str(doc, "tray", "default_open", action.as_config_str())) {
        tracing::error!(%error, "persist tray.default_open failed");
    }
}

fn toggle_close_to_tray(shared: &Shared) {
    let enabled = shared.items.close_item.is_checked().unwrap_or(false);
    shared.close_to_tray.store(enabled, Ordering::Relaxed);
    if let Err(error) = logic::persist_user_config(|doc| logic::set_bool(doc, "tray", "close_to_tray", enabled)) {
        tracing::error!(%error, enabled, "persist tray.close_to_tray failed");
    }
}

/// 有更新时聚焦主窗口；安装入口在设置 > 应用更新。
fn check_update(app: &AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = match app.updater() {
            Ok(updater) => updater.check().await.map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        match outcome {
            Ok(Some(update)) => {
                notify("kxen 有新版本", &format!("发现新版本 {}，可在 设置 > 应用更新 安装", update.version));
                show_main_window(&app);
            }
            Ok(None) => notify("kxen 已是最新版本", ""),
            Err(error) => {
                tracing::warn!(%error, "tray update check failed");
                notify("检查更新失败", &error);
            }
        }
    });
}

fn notify(title: &str, body: &str) {
    let mut notification = notify_rust::Notification::new();
    notification.summary(title);
    if !body.is_empty() {
        notification.body(body);
    }
    if let Err(error) = notification.show() {
        tracing::warn!(%error, "tray notification failed");
    }
}

fn tray_icon() -> tauri::Result<tauri::image::Image<'static>> {
    // macOS 模板图只取 alpha，深/浅菜单栏自动反色
    #[cfg(target_os = "macos")]
    {
        tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
    }
}
