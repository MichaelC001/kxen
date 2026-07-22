//! 四家真实调用验证（每家一次真实 API 调用）。

use futures::StreamExt;
use kxen_app::llm::{Delta, LlmClient, Message, ModelRef};

#[tokio::main]
async fn main() {
    let auth_path = kxen_app::core::paths::auth_file();
    let mut store = kxen_app::auth::credential::read_auth_file(&auth_path);
    kxen_app::auth::probe_all(&mut store, true);

    let cases = [
        ("anthropic", "claude-sonnet-4-5-20250929"),
        ("openai", "gpt-5.4"),
        ("xai", "grok-build-0.1"),
        ("kimi-for-coding", "kimi-for-coding"),
    ];

    for (provider, model) in cases {
        let model_ref = ModelRef::new(provider, model);
        let messages = vec![Message::user("Reply with exactly one word: pong")];
        let mut stream = LlmClient::stream(&model_ref, &messages, &store);
        let mut text = String::new();
        let mut error = String::new();
        while let Some(delta) = stream.next().await {
            match delta {
                Delta::Text(t) => text.push_str(&t),
                Delta::Error(e) => {
                    error = e;
                    break;
                }
                Delta::Done => break,
                _ => {}
            }
        }
        if error.is_empty() {
            println!("PASS  {provider:18} {model:28} -> {}", text.trim());
        } else {
            println!("FAIL  {provider:18} {model:28} -> {error}");
        }
    }
}
