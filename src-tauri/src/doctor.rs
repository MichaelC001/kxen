use kxen_app::auth::credential::AuthStore;
use kxen_app::auth::probe::RULES;
use kxen_app::core::paths;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DoctorEntry {
    pub provider: String,
    pub display: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub runtime: String,
    pub data_dir: String,
    pub config_dir: String,
    pub entries: Vec<DoctorEntry>,
}

/// 渲染当前 store 状态。探测只发生在启动后台任务（keychain 可阻塞），RPC 路径绝不触发 keychain。
/// 多账号：默认账号（官方导入）+ 命名账号各占一行。
pub fn doctor_report(store: &AuthStore) -> DoctorReport {
    let mut entries: Vec<DoctorEntry> = Vec::new();
    for rule in RULES {
        let (status, detail) = match store.get(rule.provider) {
            None => ("missing", "no credential found"),
            Some(c) if c.is_expired() => ("expired", "will refresh on next call"),
            Some(_) => ("ok", "credential present"),
        };
        entries.push(DoctorEntry {
            provider: rule.provider.to_string(),
            display: rule.display.to_string(),
            status: status.into(),
            detail: detail.into(),
        });
        // 命名账号行
        for key in kxen_account_keys(store, rule.provider) {
            let name = key.strip_prefix(&format!("{}:", rule.provider)).unwrap_or(&key);
            let (status, detail) = match store.get(&key) {
                Some(c) if c.is_expired() => ("expired", "will refresh on next call"),
                Some(_) => ("ok", "credential present"),
                None => ("missing", "no credential found"),
            };
            entries.push(DoctorEntry {
                provider: key.clone(),
                display: format!("{} · {}", rule.display, name),
                status: status.into(),
                detail: detail.into(),
            });
        }
    }
    DoctorReport {
        runtime: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: paths::data_dir().display().to_string(),
        config_dir: paths::config_dir().display().to_string(),
        entries,
    }
}

fn kxen_account_keys(store: &AuthStore, provider: &str) -> Vec<String> {
    kxen_app::auth::credential::accounts_of(store, provider)
        .into_iter()
        .filter(|k| k != provider)
        .collect()
}
