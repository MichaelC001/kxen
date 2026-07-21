//! 配置（~/.config/kxen/config.toml + 项目 .kxen/config.toml，项目级覆盖用户级）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub roles: HashMap<String, RoleBinding>,
    pub limits: Limits,
    pub hooks: HashMap<String, Vec<HookDef>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookDef {
    /// 工具名正则（None = 全部工具）。
    pub matcher: Option<String>,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleBinding {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub global_concurrent: u32,
    pub providers: HashMap<String, ProviderLimit>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            global_concurrent: 8,
            providers: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderLimit {
    pub concurrent: Option<u32>,
    pub rpm: Option<u32>,
}

impl Config {
    pub fn load(user: &Path, project: Option<&Path>) -> crate::core::Result<Self> {
        let mut config = Config::default();
        for path in [Some(user.to_path_buf()), project.map(|p| p.to_path_buf())].into_iter().flatten() {
            if !path.exists() {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let parsed: Config = toml::from_str(&text)?;
            config.merge(parsed);
        }
        Ok(config)
    }

    fn merge(&mut self, other: Config) {
        self.roles.extend(other.roles);
        if other.limits.global_concurrent != 0 {
            self.limits.global_concurrent = other.limits.global_concurrent;
        }
        self.limits.providers.extend(other.limits.providers);
        for (event, defs) in other.hooks {
            self.hooks.entry(event).or_default().extend(defs);
        }
    }
}
