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

const DEFAULT_MODELS: &[(&str, &str)] = &[
    ("anthropic", "claude-sonnet-4-5-20250929"),
    ("openai", "gpt-5.4"),
    ("xai", "grok-build-0.1"),
    ("kimi-for-coding", "kimi-for-coding"),
];

/// 发一条真实 ping：首个有效 delta 即判活；Error/超时即判死（带原始错误文案）。
pub async fn verify_provider(store: &crate::auth::credential::AuthStore, provider: &str, account: Option<&str>, model: Option<&str>) -> VerifyOutcome {
    let model_id = model
        .map(String::from)
        .or_else(|| DEFAULT_MODELS.iter().find(|(p, _)| *p == provider).map(|(_, m)| m.to_string()));
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
                Delta::Text(_) | Delta::Reasoning(_) | Delta::Usage { .. } | Delta::Done | Delta::ToolFragments(_) | Delta::ToolCall { .. } => return Ok(()),
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
