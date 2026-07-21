use kxen_app::auth::{probe_all, ProbeOutcome};
use kxen_app::core::paths;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DoctorEntry {
    provider: String,
    display: String,
    status: String,
    detail: String,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    runtime: String,
    data_dir: String,
    config_dir: String,
    entries: Vec<DoctorEntry>,
}

pub fn doctor_report() -> DoctorReport {
    let auth_path = paths::auth_file();
    let mut store = kxen_app::auth::credential::read_auth_file(&auth_path);
    let outcomes = probe_all(&mut store);
    let _ = kxen_app::auth::credential::write_auth_file(&auth_path, &store);

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
        runtime: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: paths::data_dir().display().to_string(),
        config_dir: paths::config_dir().display().to_string(),
        entries,
    }
}
