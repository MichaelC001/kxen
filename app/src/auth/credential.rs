//! 凭证类型与 auth.json 读写（0600）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialKind {
    Oauth {
        access: String,
        refresh: String,
        expires: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account_id: Option<String>,
    },
    Api {
        key: String,
    },
}

impl CredentialKind {
    pub fn expires(&self) -> Option<u64> {
        match self {
            CredentialKind::Oauth { expires, .. } => Some(*expires),
            CredentialKind::Api { .. } => None,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.is_expired_within(0)
    }

    /// buffer_ms 内将过期也算过期（提前刷新窗口）。
    pub fn is_expired_within(&self, buffer_ms: u64) -> bool {
        match self {
            CredentialKind::Oauth { expires, .. } => *expires > 0 && *expires < now_ms() + buffer_ms,
            CredentialKind::Api { .. } => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    #[serde(flatten)]
    pub kind: CredentialKind,
}

pub type AuthStore = HashMap<String, CredentialKind>;

pub fn read_auth_file(path: &Path) -> AuthStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn write_auth_file(path: &Path, store: &AuthStore) -> crate::core::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(store)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
