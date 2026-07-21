mod doctor;
mod ws;

use kxen_llm::ModelRef;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub struct AppState {
    auth_store: Mutex<kxen_auth::credential::AuthStore>,
    model: Mutex<ModelRef>,
    pub bus: kxen_core::event::EventBus,
    pub registry: std::sync::Arc<kxen_tools::task::TaskRegistry>,
    pub workdir: std::path::PathBuf,
}

impl AppState {
    #[allow(dead_code)]
    fn new() -> Self {
        let path = kxen_core::paths::auth_file();
        let mut store = kxen_auth::credential::read_auth_file(&path);
        let outcomes = kxen_auth::probe_all(&mut store);
        let _ = kxen_auth::credential::write_auth_file(&path, &store);
        for (provider, outcome, _) in &outcomes {
            tracing::info!(provider, ?outcome, "credential probe");
        }
        Self {
            auth_store: Mutex::new(store),
            model: Mutex::new(ModelRef::new("xai", "grok-build-0.1")),
            bus: kxen_core::event::EventBus::default(),
            registry: std::sync::Arc::new(kxen_tools::task::TaskRegistry::new()),
            workdir: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")),
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
            .manage(Arc::new(AppState::new()))
            .setup(|app| {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    match ws::serve(handle.clone()).await {
                        Ok(port) => {
                            tracing::info!(port, "ws server listening");
                            if let Some(window) = handle.get_webview_window("main") {
                                let _ = window.eval(&format!("window.__KXEN_WS_PORT__ = {port};"));
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
