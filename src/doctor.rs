use kxen_app::auth::credential::AuthStore;
use kxen_app::auth::probe::RULES;
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

/// 渲染当前 store 状态。探测只发生在启动后台任务（keychain 可阻塞），RPC 路径绝不触发 keychain。
pub fn doctor_report(store: &AuthStore) -> DoctorReport {
    let entries = RULES
        .iter()
        .map(|rule| {
            let (status, detail) = match store.get(rule.provider) {
                None => ("missing", "no credential found"),
                Some(c) if c.is_expired() => ("expired", "will refresh on next call"),
                Some(_) => ("ok", "credential present"),
            };
            DoctorEntry {
                provider: rule.provider.to_string(),
                display: rule.display.to_string(),
                status: status.into(),
                detail: detail.into(),
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
