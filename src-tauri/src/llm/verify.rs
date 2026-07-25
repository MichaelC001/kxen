//! 订阅实况探测：真实最小调用判定（文件新鲜 ≠ token 有效，doctor 只解决一半）。

use futures::StreamExt;
use serde::Serialize;

use crate::llm::{Delta, LlmClient, Message, ModelRef};

#[derive(Debug, Clone, Serialize)]
pub struct VerifyOutcome {
    pub ok: bool,
    pub latency_ms: u64,
    pub detail: String,
}

/// 「测试连接」的临时凭证注入：克隆 store 写入候选凭证（不落盘），
/// verify_provider 按既有账号键链路解析，候选凭证零持久化风险。
#[allow(clippy::too_many_arguments)]
pub fn store_with_temp_cred(
    store: &crate::auth::credential::AuthStore,
    provider: &str,
    account: &str,
    kind: &str,
    access: &str,
    refresh: &str,
    expires: u64,
    region: Option<&str>,
) -> crate::auth::credential::AuthStore {
    use crate::auth::credential::CredentialKind;
    let mut cloned = store.clone();
    let cred = if kind == "oauth" {
        CredentialKind::Oauth { access: access.into(), refresh: refresh.into(), expires, account_id: None }
    } else {
        // OAuth 订阅厂商全是单区域（credential.rs region()），region 只对 Api 凭证有意义
        CredentialKind::Api { key: access.into(), region: region.map(String::from) }
    };
    cloned.insert(crate::auth::credential::account_id(provider, account), cred);
    cloned
}

/// 发一条真实 ping：首个有效 delta 即判活；Error/超时即判死（带原始错误文案）。
pub async fn verify_provider(
    store: &crate::auth::credential::AuthStore,
    provider: &str,
    account: Option<&str>,
    model: Option<&str>,
) -> VerifyOutcome {
    let model_id = model.map(String::from).or_else(|| {
        if let Some(name) = provider.strip_prefix("custom:") {
            let cfg = crate::core::config::Config::load(&crate::core::paths::config_dir().join("config.toml"), None).unwrap_or_default();
            return cfg.custom_providers.get(name).and_then(|d| d.models.first().cloned());
        }
        crate::providers::find(provider).map(|s| s.default_model.to_string())
    });
    let Some(model_id) = model_id else {
        return VerifyOutcome { ok: false, latency_ms: 0, detail: format!("unknown provider: {provider}") };
    };
    let started = std::time::Instant::now();
    let model = match account {
        Some(acc) => ModelRef::with_account(provider, model_id, acc),
        None => ModelRef::new(provider, model_id),
    };
    let messages = vec![Message::user("ping, reply with one word")];
    let mut stream = LlmClient::stream(&model, &messages, store);
    let result = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while let Some(delta) = stream.next().await {
            match delta {
                Delta::Text(_)
                | Delta::Reasoning(_)
                | Delta::Usage { .. }
                | Delta::Done
                | Delta::ToolFragments(_)
                | Delta::ToolCall { .. } => return Ok(()),
                Delta::Error(e) => return Err(e),
            }
        }
        Ok(())
    })
    .await;
    let latency_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(Ok(())) => VerifyOutcome { ok: true, latency_ms, detail: "live ok".into() },
        Ok(Err(e)) => VerifyOutcome { ok: false, latency_ms, detail: e },
        Err(_) => VerifyOutcome { ok: false, latency_ms, detail: "timeout (20s)".into() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::credential::CredentialKind;

    #[test]
    fn temp_cred_lands_in_clone_only() {
        let mut store = crate::auth::credential::AuthStore::new();
        store.insert("kimi:work".into(), CredentialKind::Api { key: "old".into(), region: None });
        let probed = store_with_temp_cred(&store, "kimi", "work", "api", "new-key", "", 0, Some("intl"));
        assert!(
            matches!(&probed["kimi:work"], CredentialKind::Api { key, region } if key == "new-key" && region.as_deref() == Some("intl")),
            "临时凭证必须按账号键覆盖克隆体并带区域"
        );
        assert!(matches!(&store["kimi:work"], CredentialKind::Api { key, .. } if key == "old"), "原 store 不得被污染");
        let probed = store_with_temp_cred(&store, "anthropic", "default", "oauth", "tok", "ref", 123, None);
        assert!(
            matches!(&probed["anthropic"], CredentialKind::Oauth { access, refresh, expires, .. } if access == "tok" && refresh == "ref" && *expires == 123),
            "oauth 形态必须保留 refresh/expires"
        );
    }
}
