//! 流式调用重试：429/5xx/网络类错误指数退避 + 同 provider 账号池轮换。
//! 只在「本轮尚未产出任何内容」时重试——部分产出后重试会重复文本，直接终态。

use crate::auth::credential::AuthStore;

pub const MAX_ATTEMPTS: usize = 3;

/// 可重试的错误类：限流 / 5xx / 网络断连。401/403（凭证）与业务错误不重试。
pub fn retryable(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    if e.contains("401") || e.contains("403") {
        return false;
    }
    e.contains("429")
        || e.contains("rate limit")
        || e.contains("rate_limit")
        || e.contains("500")
        || e.contains("502")
        || e.contains("503")
        || e.contains("504")
        || e.contains("timeout")
        || e.contains("timed out")
        || e.contains("connect")
        || e.contains("eof")
        || e.contains("reset")
        || e.contains("request failed")
}

/// 指数退避 + 抖动：800ms / 1.6s / 3.2s 起步。
pub fn backoff_ms(attempt: usize) -> u64 {
    let base = 800u64 << attempt.min(3);
    let jitter = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| (d.subsec_millis() % 500) as u64).unwrap_or(0);
    base + jitter
}

/// 账号池轮换：同 provider 下一个账号名（"default"=裸 provider 账号；无备选返回 None）。
pub fn next_account(store: &AuthStore, provider: &str, current: Option<&str>) -> Option<String> {
    let effective = current.unwrap_or("default");
    crate::auth::credential::accounts_of(store, provider)
        .into_iter()
        .map(|k| k.strip_prefix(&format!("{provider}:")).map(String::from).unwrap_or_else(|| "default".into()))
        .find(|name| name != effective)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_classification() {
        assert!(retryable("xai HTTP 429: too many requests"));
        assert!(retryable("HTTP 503: upstream unavailable"));
        assert!(retryable("request failed: connection reset"));
        assert!(retryable("operation timed out"));
        assert!(!retryable("HTTP 401: unauthorized"));
        assert!(!retryable("missing command"));
    }

    #[test]
    fn backoff_grows() {
        assert!(backoff_ms(0) >= 800 && backoff_ms(0) < 1300);
        assert!(backoff_ms(1) >= 1600);
        assert!(backoff_ms(2) >= 3200);
    }

    #[test]
    fn next_account_rotates() {
        let mut store = AuthStore::default();
        store.insert("xai".into(), crate::auth::credential::CredentialKind::Api { key: "k1".into(), region: None });
        store.insert("xai:work".into(), crate::auth::credential::CredentialKind::Api { key: "k2".into(), region: None });
        assert_eq!(next_account(&store, "xai", None).as_deref(), Some("work"));
        assert_eq!(next_account(&store, "xai", Some("work")).as_deref(), Some("default"));
        assert_eq!(next_account(&store, "openai", None), None);
        // 单账号无备选
        let mut single = AuthStore::default();
        single.insert("xai".into(), crate::auth::credential::CredentialKind::Api { key: "k".into(), region: None });
        assert_eq!(next_account(&single, "xai", None), None);
    }
}
