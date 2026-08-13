use super::*;

fn seeded_mrm() -> ModelResourceManager {
    // 不存在路径 = 纯默认种子（load 内部 seed_default_roles）
    let config = Config::load(std::path::Path::new("/nonexistent-kxen-mrm-test/config.toml"), None).expect("default config");
    ModelResourceManager::new(config)
}

/// roles.chat 默认种子存在，空凭证库（首启探测前）下 resolve 命中盲默认键。
#[tokio::test]
async fn seeded_chat_role_resolves() {
    let mrm = seeded_mrm();
    let store = crate::auth::credential::AuthStore::default();
    let resolved = mrm.resolve("chat", &store).await.expect("chat 种子必须可 resolve");
    assert_eq!((resolved.provider.as_str(), resolved.model.as_str()), ("xai", "grok-build-0.1"));
}

/// peek 与 resolve 同序结论但不写派发历史：轮询路径不得污染 mrm.stats。
#[tokio::test]
async fn peek_resolves_without_recording_history() {
    let mrm = seeded_mrm();
    let store = crate::auth::credential::AuthStore::default();
    let peeked = mrm.peek("chat", &store).await.expect("peek 必须命中");
    assert_eq!(peeked.model, "grok-build-0.1");
    assert!(mrm.history().await.is_empty(), "peek 不得记录派发历史");
    assert!(mrm.resolve("chat", &store).await.is_some());
    assert_eq!(mrm.history().await.len(), 1, "resolve 保持记录语义");
}

#[tokio::test]
async fn local_free_provider_resolves_with_unrelated_credentials_present() {
    let mut config = Config::default();
    config.roles.insert(
        "execution".into(),
        crate::core::config::RoleBinding { provider: "ollama".into(), model: "qwen".into(), ..Default::default() },
    );
    let mrm = ModelResourceManager::new(config);
    let mut store = crate::auth::credential::AuthStore::default();
    store.insert("openai".into(), crate::auth::credential::CredentialKind::Api { key: "unrelated".into(), region: None });

    let resolved = mrm.resolve("execution", &store).await.expect("local-free provider needs no credential");

    assert_eq!(resolved.provider, "ollama");
    assert!(resolved.account.is_none());
}

fn readiness_config(provider: &str) -> Config {
    let mut config = Config::default();
    for role in std::iter::once("chat").chain(REQUIRED_AGENT_ROLES) {
        config.roles.insert(
            role.into(),
            crate::core::config::RoleBinding { provider: provider.into(), model: "model".into(), ..Default::default() },
        );
    }
    config
}

#[tokio::test]
async fn readiness_accepts_local_free_routes_without_credentials() {
    let mrm = ModelResourceManager::new(readiness_config("ollama"));
    let report = mrm.readiness(&Default::default()).await;

    assert!(report.chat_ready);
    assert!(report.agents_ready);
    assert!(report.all_ready);
    assert!(report.roles.iter().all(|role| role.status == RouteReadinessStatus::Ready));
    assert!(mrm.history().await.is_empty(), "readiness must not record dispatch history");
}

#[tokio::test]
async fn readiness_does_not_treat_missing_remote_credentials_as_ready() {
    let mrm = ModelResourceManager::new(readiness_config("xai"));
    let report = mrm.readiness(&Default::default()).await;

    assert!(!report.chat_ready);
    assert!(!report.agents_ready);
    assert!(report.roles.iter().all(|role| role.status == RouteReadinessStatus::MissingCredential));
    assert!(mrm.history().await.is_empty());
}
