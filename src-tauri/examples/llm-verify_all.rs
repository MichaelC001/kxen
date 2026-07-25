//! 九家真实调用验证（每家一次真实 API 调用）。

use futures::StreamExt;
use kxen_app::llm::{Delta, LlmClient, Message, ModelRef};

#[tokio::main]
async fn main() {
    let auth_path = kxen_app::core::paths::auth_file();
    let mut store = kxen_app::auth::credential::read_auth_file(&auth_path);
    kxen_app::auth::probe_all(&mut store, true);

    // model 与 providers registry 的 default_model 对齐
    let cases = [
        ("anthropic", "claude-sonnet-4-5-20250929"),
        ("openai", "gpt-5.4"),
        ("xai", "grok-build-0.1"),
        ("kimi-for-coding", "kimi-for-coding"),
        ("deepseek", "deepseek-chat"),
        ("mistral", "mistral-large-latest"),
        ("groq", "llama-3.3-70b-versatile"),
        ("google", "gemini-2.5-flash"),
        ("together", "meta-llama/Llama-3.3-70B-Instruct-Turbo"),
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
