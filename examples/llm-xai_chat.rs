//! xai 真实流式调用验证（用 probe 导入的凭证）。

use futures::StreamExt;
use kxen_app::llm::{Delta, LlmClient, Message, ModelRef};

#[tokio::main]
async fn main() {
    let auth_path = kxen_app::core::paths::auth_file();
    let mut store = kxen_app::auth::credential::read_auth_file(&auth_path);
    let outcomes = kxen_app::auth::probe_all(&mut store, true);
    for (p, o, _) in &outcomes {
        eprintln!("probe {p}: {o:?}");
    }

    let model = ModelRef::new("xai", "grok-build-0.1");
    let messages = vec![Message::user("Reply with exactly one word: pong")];
    let mut stream = LlmClient::stream(&model, &messages, &store);

    while let Some(delta) = stream.next().await {
        match delta {
            Delta::Text(t) => print!("{t}"),
            Delta::Reasoning(r) => eprint!("[r:{r}]"),
            Delta::Usage { input, output } => eprintln!("\nusage: in={input} out={output}"),
            Delta::Done => eprintln!("\n[done]"),
            Delta::Error(e) => eprintln!("\n[error] {e}"),
            Delta::ToolCall { .. } => {}
            Delta::ToolFragments(_) => {}
        }
    }
}
