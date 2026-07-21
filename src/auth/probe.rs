//! 四源订阅探测（当前规则：Claude / Codex / Grok / Kimi）。
//! 每规则：读官方 CLI 凭证存储 -> 与现有 auth.json 条目比新鲜度（expires 大者优先）。

use crate::auth::credential::{AuthStore, CredentialKind};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// 官方源更新（或首次）导入
    Imported,
    /// 现有条目已是最新
    Fresh,
    /// 官方源不存在；现有条目保留（若有）
    Missing,
}

pub struct ProbeRule {
    pub provider: &'static str,
    pub display: &'static str,
    probe: fn() -> Option<CredentialKind>,
    /// 环境变量覆盖（开发期暂存，免官方源访问）
    env_override: Option<&'static str>,
}

pub const RULES: &[ProbeRule] = &[
    ProbeRule { provider: "anthropic", display: "Claude Pro/Max", probe: probe_claude, env_override: Some("KXEN_CLAUDE_OAUTH") },
    ProbeRule { provider: "openai", display: "ChatGPT Plus/Pro (codex)", probe: probe_codex, env_override: None },
    ProbeRule { provider: "xai", display: "SuperGrok (grok-build)", probe: probe_grok, env_override: None },
    ProbeRule { provider: "kimi-for-coding", display: "Kimi Code", probe: probe_kimi, env_override: None },
];

/// 单规则探测带 5s 超时：keychain ACL 弹窗会无限阻塞调用线程（macOS 未签名二进制），
/// 超时视为不可得，保住其余规则的导入与 app 启动。
fn probe_with_timeout(rule: &ProbeRule) -> Option<CredentialKind> {
    let probe = rule.probe;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(probe());
    });
    rx.recv_timeout(std::time::Duration::from_secs(5)).ok().flatten()
}

const TEN_YEARS_MS: u64 = 10 * 365 * 24 * 3600 * 1000;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// expires 单位归一（ms）。kimi 官方文件是秒级；历史代码无差别 *1000 会产生荒诞远期值。
fn sane_expires(v: u64) -> u64 {
    if v > 1_000_000_000_000 { v } else { v * 1000 }
}

/// 荒诞远期 expires（单位 bug 产物）按已过期处理，让 store 在下轮探测自修复。
fn poisoned(c: &CredentialKind) -> bool {
    matches!(c.expires(), Some(v) if v > now_ms() + TEN_YEARS_MS)
}

/// 全源探测：返回 (provider, outcome, display)。store 就地更新。
pub fn probe_all(store: &mut AuthStore) -> Vec<(&'static str, ProbeOutcome, &'static str)> {
    RULES
        .iter()
        .map(|rule| {
            // 自有存储为 oauth 且未在刷新窗口（30min）内才豁免官方源（避免反复授权弹窗）；
            // Api 类型无过期信息，每次必须重新评估（kimi 轮换场景）
            let existing = store.get(rule.provider);
            let exempt = matches!(existing, Some(CredentialKind::Oauth { .. }))
                && existing.is_some_and(|c| !poisoned(c) && !c.is_expired_within(30 * 60 * 1000));
            if exempt {
                return (rule.provider, ProbeOutcome::Fresh, rule.display);
            }
            // env override（开发期暂存，最高优先）
            let imported = rule.env_override.and_then(|var| read_env_override(var)).or_else(|| probe_with_timeout(rule));
            let outcome = match imported {
                None => {
                    if store.contains_key(rule.provider) { ProbeOutcome::Fresh } else { ProbeOutcome::Missing }
                }
                Some(new) => {
                    let existing_stale = store.get(rule.provider).is_some_and(poisoned);
                    let fresher = existing_stale
                        || match store.get(rule.provider) {
                            None => true,
                            Some(existing) => new.expires().unwrap_or(u64::MAX) > existing.expires().unwrap_or(u64::MAX),
                        };
                    if fresher {
                        store.insert(rule.provider.to_string(), new);
                        ProbeOutcome::Imported
                    } else {
                        ProbeOutcome::Fresh
                    }
                }
            };
            (rule.provider, outcome, rule.display)
        })
        .collect()
}

fn read_env_override(var: &str) -> Option<CredentialKind> {
    let raw = std::env::var(var).ok()?;
    let raw = raw.strip_prefix("file://").map(|p| std::fs::read_to_string(p).ok()).unwrap_or(Some(raw.to_string()))?;
    parse_claude(raw.trim())
}

// --- Claude（Keychain 优先，~/.claude/.credentials.json 兜底） ---

#[derive(Deserialize)]
struct ClaudeCredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOauth>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOauth {
    access_token: String,
    refresh_token: String,
    expires_at: u64,
}

fn probe_claude() -> Option<CredentialKind> {
    // macOS：官方 CLI 默认写 Keychain（service: Claude Code-credentials，account: 本机用户名）
    let account = std::env::var("USER").unwrap_or_default();
    for acct in [account.as_str(), "claude"] {
        if acct.is_empty() {
            continue;
        }
        if let Ok(entry) = keyring::Entry::new("Claude Code-credentials", acct) {
            if let Ok(raw) = entry.get_password() {
                if let Some(cred) = parse_claude(&raw) {
                    return Some(cred);
                }
            }
        }
    }
    // 兜底：凭证 JSON 文件（Linux/Windows 形态，或手动放置）
    let file = home()?.join(".claude/.credentials.json");
    let raw = std::fs::read_to_string(file).ok()?;
    parse_claude(&raw)
}

fn parse_claude(raw: &str) -> Option<CredentialKind> {
    let parsed: ClaudeCredentialsFile = serde_json::from_str(raw).ok()?;
    let oauth = parsed.claude_ai_oauth?;
    Some(CredentialKind::Oauth {
        access: oauth.access_token,
        refresh: oauth.refresh_token,
        expires: oauth.expires_at,
        account_id: None,
    })
}

// --- Codex（~/.codex/auth.json） ---

#[derive(Deserialize)]
struct CodexAuthFile {
    tokens: Option<CodexTokens>,
}

#[derive(Deserialize)]
struct CodexTokens {
    access_token: String,
    refresh_token: String,
    account_id: Option<String>,
}

fn probe_codex() -> Option<CredentialKind> {
    let file = home()?.join(".codex/auth.json");
    let raw = std::fs::read_to_string(file).ok()?;
    let parsed: CodexAuthFile = serde_json::from_str(&raw).ok()?;
    let t = parsed.tokens?;
    let expires = jwt_exp(&t.access_token).unwrap_or(0);
    Some(CredentialKind::Oauth {
        access: t.access_token,
        refresh: t.refresh_token,
        expires,
        account_id: t.account_id,
    })
}

// --- Grok（~/.grok/auth.json，issuer map 取 expires 最新） ---

#[derive(Deserialize)]
struct GrokEntry {
    key: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<serde_json::Value>,
}

fn probe_grok() -> Option<CredentialKind> {
    let file = home()?.join(".grok/auth.json");
    let raw = std::fs::read_to_string(file).ok()?;
    let map: std::collections::HashMap<String, GrokEntry> = serde_json::from_str(&raw).ok()?;
    let mut best: Option<(String, String, u64)> = None;
    for entry in map.values() {
        let Some(key) = entry.key.clone() else { continue };
        let expires = parse_expires(entry.expires_at.as_ref());
        if best.as_ref().is_none_or(|(_, _, e)| expires > *e) {
            best = Some((key, entry.refresh_token.clone().unwrap_or_default(), expires));
        }
    }
    let (key, refresh, expires) = best?;
    Some(CredentialKind::Oauth { access: key, refresh, expires, account_id: None })
}

fn parse_expires(value: Option<&serde_json::Value>) -> u64 {
    match value {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(serde_json::Value::String(s)) => {
            // ISO 8601 -> ms（粗解析：取前 19 位按 UTC）
            chrono_free_iso_ms(s).unwrap_or(0)
        }
        _ => 0,
    }
}

fn chrono_free_iso_ms(s: &str) -> Option<u64> {
    // 简化：用 time crate 的 OffsetDateTime 解析 RFC3339
    let t = time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()?;
    Some((t.unix_timestamp_nanos() / 1_000_000) as u64)
}

// --- Kimi（~/.kimi-code/credentials/kimi-code.json，Bearer 直连作 api key） ---

#[derive(Deserialize)]
struct KimiCredentials {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
}

fn probe_kimi() -> Option<CredentialKind> {
    let file = home()?.join(".kimi-code/credentials/kimi-code.json");
    let raw = std::fs::read_to_string(file).ok()?;
    let parsed: KimiCredentials = serde_json::from_str(&raw).ok()?;
    // kimi 官方文件是 oauth 形态（access/refresh/expires_at）——保留过期时间才能正确轮换；单位归一防荒诞远期
    Some(CredentialKind::Oauth {
        access: parsed.access_token?,
        refresh: parsed.refresh_token.unwrap_or_default(),
        expires: parsed.expires_at.map(sane_expires).unwrap_or(0),
        account_id: None,
    })
}

// --- 工具 ---

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// JWT exp（秒）-> ms；解析失败返回 None。
fn jwt_exp(token: &str) -> Option<u64> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64_url_decode(payload)?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    Some(json.get("exp")?.as_u64()? * 1000)
}

fn base64_url_decode(input: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(input).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresher_wins() {
        let mut store = AuthStore::new();
        store.insert("x".into(), CredentialKind::Oauth { access: "old".into(), refresh: String::new(), expires: 100, account_id: None });
        let new = CredentialKind::Oauth { access: "new".into(), refresh: String::new(), expires: 200, account_id: None };
        let fresher = new.expires().unwrap_or(u64::MAX) > store["x"].expires().unwrap_or(u64::MAX);
        assert!(fresher);
    }

    #[test]
    fn jwt_exp_parses() {
        // exp = 2000000000
        let token = "x.eyJleHAiOjIwMDAwMDAwMDB9.y";
        assert_eq!(jwt_exp(token), Some(2000000000_000));
    }
}
