use kxen_app::auth::credential::AuthStore;
use kxen_app::auth::probe::RULES;
use kxen_app::core::paths;
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

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
    /// 子系统健康（MCP/LSP/MRM/event bus）：仅 RPC 路径填（需 AppState），reprobe 纯凭证路径为 None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemHealth>,
}

#[derive(Debug, Serialize)]
pub struct LspHealth {
    pub language: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct SystemHealth {
    pub mcp: Vec<kxen_app::mcp::ServerStatus>,
    pub lsp_root: String,
    pub lsp: Vec<LspHealth>,
    pub mrm_describe: String,
    pub mrm_dispatches: usize,
    pub bus_capacity: usize,
    pub bus_receivers: usize,
}

/// 子系统健康汇总：各 manager 现有 status/describe API 的只读拼装，不触发任何启动/连接动作。
pub async fn system_health(state: &Arc<AppState>) -> SystemHealth {
    let mcp = state.mcp.status();
    let (lsp_root, lsp) = {
        let lsp = state.lsp.read().expect("lsp").clone();
        let root = lsp.root().to_string_lossy().into_owned();
        let entries = lsp.status().await.into_iter().map(|(language, status)| LspHealth { language, status }).collect();
        (root, entries)
    };
    let (mrm_describe, mrm_dispatches) = {
        let mrm = state.mrm.read().expect("mrm").clone();
        (mrm.describe().await, mrm.history().await.len())
    };
    let (bus_capacity, bus_receivers) = state.bus.stats();
    SystemHealth { mcp, lsp_root, lsp, mrm_describe, mrm_dispatches, bus_capacity, bus_receivers }
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
        system: None,
    }
}

fn kxen_account_keys(store: &AuthStore, provider: &str) -> Vec<String> {
    kxen_app::auth::credential::accounts_of(store, provider).into_iter().filter(|k| k != provider).collect()
}
