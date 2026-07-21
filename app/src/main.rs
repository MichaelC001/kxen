use futures::StreamExt;
use kxen_llm::{Delta, LlmClient, Message, ModelRef};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

mod doctor;

#[derive(Default)]
struct AppState {
    auth_store: Mutex<kxen_auth::credential::AuthStore>,
    model: Mutex<ModelRef>,
}

impl AppState {
    fn new() -> Self {
        let path = kxen_core::paths::auth_file();
        let mut store = kxen_auth::credential::read_auth_file(&path);
        let outcomes = kxen_auth::probe_all(&mut store);
        let _ = kxen_auth::credential::write_auth_file(&path, &store);
        for (provider, outcome, _) in &outcomes {
            tracing::info!(provider, ?outcome, "credential probe");
        }
        Self {
            auth_store: Mutex::new(store),
            model: Mutex::new(ModelRef::new("xai", "grok-build-0.1")),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LlmEvent {
    Text { text: String },
    Reasoning { text: String },
    Usage { input: u64, output: u64 },
    Done,
    Error { message: String },
}

#[derive(Deserialize)]
struct SendInput {
    text: String,
    #[serde(default)]
    history: Vec<HistoryMessage>,
}

#[derive(Deserialize, Serialize, Clone)]
struct HistoryMessage {
    role: String,
    content: String,
}

#[tauri::command]
async fn send_message(app: AppHandle, state: State<'_, AppState>, input: SendInput) -> Result<(), String> {
    let (model, store) = {
        let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
        (state.model.lock().map_err(|e| e.to_string())?.clone(), store)
    };

    let mut messages: Vec<Message> = input
        .history
        .iter()
        .map(|m| match m.role.as_str() {
            "system" => Message::system(m.content.clone()),
            "assistant" => Message::assistant(m.content.clone()),
            _ => Message::user(m.content.clone()),
        })
        .collect();
    messages.push(Message::user(input.text));

    let mut stream = LlmClient::stream(&model, &messages, &store);
    while let Some(delta) = stream.next().await {
        let event = match delta {
            Delta::Text(text) => LlmEvent::Text { text },
            Delta::Reasoning(text) => LlmEvent::Reasoning { text },
            Delta::Usage { input, output } => LlmEvent::Usage { input, output },
            Delta::Done => LlmEvent::Done,
            Delta::Error(message) => LlmEvent::Error { message },
            Delta::ToolCall { .. } => continue,
        };
        app.emit("llm://delta", event).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn current_model(state: State<'_, AppState>) -> ModelRef {
    state.model.lock().expect("model").clone()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![doctor::doctor, send_message, current_model])
        .run(tauri::generate_context!())
        .expect("error while running kxen");
}

fn main() {
    run();
}
