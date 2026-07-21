use kxen_auth::{probe_all, ProbeOutcome};
use kxen_core::paths;
use serde::Serialize;

#[derive(Serialize)]
struct DoctorEntry {
    provider: String,
    display: String,
    status: String,
    detail: String,
}

#[derive(Serialize)]
struct DoctorReport {
    bun_like_runtime: String,
    data_dir: String,
    config_dir: String,
    entries: Vec<DoctorEntry>,
}

#[tauri::command]
fn doctor() -> DoctorReport {
    let auth_path = paths::auth_file();
    let mut store = kxen_auth::credential::read_auth_file(&auth_path);
    let outcomes = probe_all(&mut store);
    let _ = kxen_auth::credential::write_auth_file(&auth_path, &store);

    let entries = outcomes
        .iter()
        .map(|(provider, outcome, display)| {
            let (status, detail) = match outcome {
                ProbeOutcome::Imported => ("imported", "updated from official CLI"),
                ProbeOutcome::Fresh => ("ok", "credential present"),
                ProbeOutcome::Missing => ("missing", "no credential found"),
            };
            let expired = store.get(*provider).is_some_and(|c| c.is_expired());
            DoctorEntry {
                provider: provider.to_string(),
                display: display.to_string(),
                status: if expired { "expired".into() } else { status.into() },
                detail: if expired { "will refresh on next call".into() } else { detail.into() },
            }
        })
        .collect();

    DoctorReport {
        bun_like_runtime: format!("rust {}", env!("CARGO_PKG_RUST_VERSION", "1.96")),
        data_dir: paths::data_dir().display().to_string(),
        config_dir: paths::config_dir().display().to_string(),
        entries,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![doctor])
        .run(tauri::generate_context!())
        .expect("error while running kxen");
}

fn main() {
    run();
}
