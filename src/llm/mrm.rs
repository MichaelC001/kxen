//! mrm（全局模型资源管理）：角色路由 + per-provider 并发 semaphore + RPM 滑窗 + 降级链。
//! 一切 LLM 调用与 subagent 派发经 acquire/release（RAII guard 自然释放）。

use crate::core::config::{Config, RoleBinding};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

pub struct ModelResourceManager {
    config: Config,
    semaphores: Mutex<HashMap<String, Arc<Semaphore>>>,
    rpm_windows: Mutex<HashMap<String, Vec<Instant>>>,
    history: Mutex<std::collections::VecDeque<DispatchRecord>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DispatchRecord {
    pub role: String,
    pub provider: String,
    pub model: String,
    pub account: Option<String>,
    pub degraded_from: Option<String>,
    pub at: u64,
}

pub struct Slot {
    _permit_global: OwnedSemaphorePermit,
    _permit_provider: OwnedSemaphorePermit,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub provider: String,
    pub model: String,
    /// 命中的账号（None = 默认账号；多账号轮转的证据）
    pub account: Option<String>,
    pub degraded_from: Option<String>,
}

impl Resolved {
    /// 限流键（account_id 体系：默认账号 = 裸 provider）。
    pub fn slot_key(&self) -> String {
        crate::auth::credential::account_id(&self.provider, self.account.as_deref().unwrap_or("default"))
    }
}

impl ModelResourceManager {
    pub fn new(config: Config) -> Self {
        Self { config, semaphores: Mutex::new(HashMap::new()), rpm_windows: Mutex::new(HashMap::new()), history: Mutex::new(std::collections::VecDeque::new()) }
    }

    pub fn role(&self, role: &str) -> Option<&RoleBinding> {
        self.config.roles.get(role)
    }

    /// 角色 -> 可执行 provider/model/account（钉账号优先；否则账号链轮转找第一个有槽的）。
    pub async fn resolve(&self, role: &str, store: &crate::auth::credential::AuthStore) -> Option<Resolved> {
        let chain = self.role_chain(role);
        let mut first = true;
        for r in chain {
            let binding = self.config.roles.get(&r)?;
            let degraded_from = if first { None } else { Some(role.to_string()) };
            // 钉账号：只看该账号（不可用则走链下一环）
            if let Some(acc) = &binding.account {
                let key = crate::auth::credential::account_id(&binding.provider, acc);
                if store.contains_key(&key) && self.available(&key).await {
                    let resolved = Resolved { provider: binding.provider.clone(), model: binding.model.clone(), account: Some(acc.clone()), degraded_from };
                    self.record(role, &resolved).await;
                    return Some(resolved);
                }
                first = false;
                continue;
            }
            // 账号链轮转：默认 -> 命名字典序，首个有凭证且有槽
            let mut hit: Option<(String, Option<String>)> = None;
            for key in crate::auth::credential::accounts_of(store, &binding.provider) {
                if self.available(&key).await {
                    let account = key.strip_prefix(&format!("{}:", binding.provider)).map(String::from);
                    hit = Some((key, account));
                    break;
                }
            }
            // 无账号线索时退回默认键（保留原始行为：限流不看凭证在否）
            if hit.is_none() && self.available(&binding.provider).await {
                hit = Some((binding.provider.clone(), None));
            }
            if let Some((_key, account)) = hit {
                let resolved = Resolved { provider: binding.provider.clone(), model: binding.model.clone(), account, degraded_from };
                self.record(role, &resolved).await;
                return Some(resolved);
            }
            first = false;
        }
        None
    }

    async fn record(&self, role: &str, resolved: &Resolved) {
        let mut history = self.history.lock().await;
        history.push_back(DispatchRecord {
            role: role.to_string(),
            provider: resolved.provider.clone(),
            model: resolved.model.clone(),
            account: resolved.account.clone(),
            degraded_from: resolved.degraded_from.clone(),
            at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
        });
        if history.len() > 50 {
            history.pop_front();
        }
    }

    /// 派发历史（新->旧）。
    pub async fn history(&self) -> Vec<DispatchRecord> {
        self.history.lock().await.iter().rev().cloned().collect()
    }

    fn role_chain(&self, role: &str) -> Vec<String> {
        // 未绑定角色（如 observer）回落 execution，避免 teammate spawn 因角色未配置直接失败
        if !self.config.roles.contains_key(role) && self.config.roles.contains_key("execution") {
            return vec!["execution".to_string()];
        }
        // config 化兜底链：binding.fallback 单跳（链式递归取），缺省走静态链
        let mut chain = vec![role.to_string()];
        let mut cursor = role.to_string();
        let mut hops = 0;
        while hops < 3 {
            let Some(next) = self.config.roles.get(&cursor).and_then(|b| b.fallback.clone()) else { break };
            if chain.contains(&next) {
                break;
            }
            chain.push(next.clone());
            cursor = next;
            hops += 1;
        }
        if chain.len() > 1 {
            return chain;
        }
        // 静态兜底（无 config fallback 时）
        let fallback: &[&str] = match role {
            "thinking" => &["planning", "research"],
            "planning" => &["thinking", "research"],
            "review" => &["thinking", "research"],
            _ => &[],
        };
        for f in fallback {
            if self.config.roles.contains_key(*f) {
                chain.push((*f).to_string());
            }
        }
        chain
    }

    async fn semaphore_for(&self, provider: &str) -> Arc<Semaphore> {
        let limit = self.limit_of(provider) as usize;
        let mut map = self.semaphores.lock().await;
        map.entry(provider.to_string()).or_insert_with(|| Arc::new(Semaphore::new(limit.max(1)))).clone()
    }

    fn limit_of(&self, key: &str) -> u32 {
        // key 可为账号槽位键（provider:account）：取 provider 段的限额配置
        let provider = key.split(':').next().unwrap_or(key);
        self.config
            .limits
            .providers
            .get(provider)
            .and_then(|l| l.concurrent)
            .unwrap_or(self.config.limits.global_concurrent.max(1))
    }

    pub async fn available(&self, provider: &str) -> bool {
        let sem = self.semaphore_for(provider).await;
        sem.available_permits() > 0
    }

    /// 占槽（并发 semaphore + RPM 滑窗等待），返回 RAII guard。
    pub async fn acquire(&self, provider: &str) -> Slot {
        self.wait_rpm(provider).await;
        let sem = self.semaphore_for(provider).await;
        let permit_provider = sem
            .acquire_owned()
            .await
            .expect("semaphore closed");
        // 全局并发（用 global_concurrent 总量的独立 semaphore）
        let global = self.semaphore_for("").await;
        let permit_global = global.acquire_owned().await.expect("semaphore closed");
        Slot { _permit_global: permit_global, _permit_provider: permit_provider }
    }

    async fn wait_rpm(&self, key: &str) {
        let provider = key.split(':').next().unwrap_or(key);
        let rpm = match self.config.limits.providers.get(provider).and_then(|l| l.rpm) {
            Some(r) if r > 0 => r,
            _ => return,
        };
        loop {
            let wait_ms = {
                let mut windows = self.rpm_windows.lock().await;
                let window = windows.entry(key.to_string()).or_default();
                let cutoff = Instant::now() - Duration::from_secs(60);
                window.retain(|t| *t > cutoff);
                if (window.len() as u32) < rpm {
                    window.push(Instant::now());
                    0
                } else {
                    let oldest = window[0];
                    60_000u64.saturating_sub(oldest.elapsed().as_millis() as u64)
                }
            };
            if wait_ms == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
        }
    }

    pub async fn describe(&self) -> String {
        let map = self.semaphores.lock().await;
        let mut lines = vec![format!("global limit: {}", self.config.limits.global_concurrent)];
        for (provider, sem) in map.iter() {
            lines.push(format!("{provider}: {}/{} available", sem.available_permits(), self.limit_of(provider)));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{Limits, ProviderLimit, RoleBinding};
    use std::collections::HashMap;

    fn config() -> Config {
        let mut roles = HashMap::new();
        roles.insert("thinking".into(), RoleBinding { provider: "anthropic".into(), model: "claude".into(), fallback: None, account: None });
        roles.insert("execution".into(), RoleBinding { provider: "xai".into(), model: "grok".into(), fallback: None, account: None });
        roles.insert("planning".into(), RoleBinding { provider: "xai".into(), model: "grok".into(), fallback: None, account: None });
        Config {
            roles,
            limits: Limits {
                global_concurrent: 2,
                providers: [("anthropic".into(), ProviderLimit { concurrent: Some(1), rpm: None })].into_iter().collect(),
            },
            hooks: HashMap::new(),
            statusline: Default::default(),
            voice: Default::default(),
        }
    }

    fn store() -> crate::auth::credential::AuthStore {
        crate::auth::credential::AuthStore::new()
    }

    #[tokio::test]
    async fn resolve_and_degrade() {
        let mrm = ModelResourceManager::new(config());
        let r = mrm.resolve("thinking", &store()).await.unwrap();
        assert_eq!(r.provider, "anthropic");
        assert!(r.degraded_from.is_none());

        let slot = mrm.acquire("anthropic").await;
        let r2 = mrm.resolve("thinking", &store()).await.unwrap();
        assert_eq!(r2.provider, "xai");
        assert_eq!(r2.degraded_from.as_deref(), Some("thinking"));
        drop(slot);
    }

    #[tokio::test]
    async fn unbound_role_falls_back_to_execution() {
        let mrm = ModelResourceManager::new(config());
        let r = mrm.resolve("observer", &store()).await.expect("observer 应回落 execution");
        assert_eq!(r.provider, "xai");
    }

    #[tokio::test]
    async fn multi_account_rotation_and_pin() {
        use crate::auth::credential::CredentialKind;
        let mut roles = HashMap::new();
        roles.insert("execution".into(), RoleBinding { provider: "xai".into(), model: "grok".into(), fallback: None, account: None });
        roles.insert("pinned".into(), RoleBinding { provider: "xai".into(), model: "grok".into(), fallback: None, account: Some("b".into()) });
        let config = Config {
            roles,
            limits: Limits { global_concurrent: 8, providers: [("xai".into(), ProviderLimit { concurrent: Some(1), rpm: None })].into_iter().collect() },
            hooks: HashMap::new(),
            statusline: Default::default(),
            voice: Default::default(),
        };
        let mrm = ModelResourceManager::new(config);
        let mut store = store();
        store.insert("xai".into(), CredentialKind::Api { key: "k0".into() });
        store.insert("xai:b".into(), CredentialKind::Api { key: "k1".into() });

        // 轮转：默认账号占满后命中 xai:b
        let slot = mrm.acquire("xai").await;
        let r = mrm.resolve("execution", &store).await.unwrap();
        assert_eq!(r.account.as_deref(), Some("b"));
        assert_eq!(r.slot_key(), "xai:b");
        drop(slot);

        // 钉账号：始终 xai:b
        let r2 = mrm.resolve("pinned", &store).await.unwrap();
        assert_eq!(r2.account.as_deref(), Some("b"));
        // 钉的账号缺凭证：不落它，走 fallback（这里无 fallback -> None）
        store.remove("xai:b");
        assert!(mrm.resolve("pinned", &store).await.is_none());
    }

    #[tokio::test]
    async fn acquire_blocks_at_limit() {
        let mrm = Arc::new(ModelResourceManager::new(config()));
        let s1 = mrm.acquire("anthropic").await;
        assert!(!mrm.available("anthropic").await);
        let mrm2 = mrm.clone();
        let handle = tokio::spawn(async move { mrm2.acquire("anthropic").await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!handle.is_finished());
        drop(s1);
        let _s2 = handle.await.unwrap();
    }
}
