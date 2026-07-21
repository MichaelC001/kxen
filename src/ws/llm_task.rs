//! LLM 任务：send_message 触发的 agent run。

use kxen_app::llm::Message;
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::AppState;

pub(super) async fn run_llm(session_id: String, text: String, context: Vec<kxen_app::agent::context::ContextItem>, images: Vec<kxen_app::llm::types::ImagePart>, app: AppHandle) {
    use kxen_app::core::session as ses;

    let state = app.state::<Arc<AppState>>();
    let sessions_dir = kxen_app::core::paths::sessions_dir();

    // @ 引用注入：chip -> 上下文块（文件/目录/Web/Docs），追加在用户消息尾部
    let context_block = if context.is_empty() {
        String::new()
    } else {
        kxen_app::agent::context::build_context(&context, &state.workdir).await
    };
    let text = if context_block.is_empty() { text } else { format!("{text}\n{context_block}") };

    // 用户消息落盘（LLM 历史以后端会话存储为准，前端不再传 history）
    let user_msg = ses::new_message(&session_id, ses::Role::User, vec![ses::Part::Text { text: text.clone() }]);
    let with_images = !images.is_empty();
    if let Err(e) = ses::append_message(&sessions_dir, &user_msg) {
        tracing::error!(error = %e, "session append failed");
        return;
    }

    let (model, store, registry, workdir, bus) = {
        let store = state.auth_store.lock().map(|s| s.clone()).unwrap_or_default();
        (
            state.model.lock().map(|m| m.clone()).unwrap_or_default(),
            store,
            state.registry.clone(),
            state.workdir.clone(),
            state.bus.clone(),
        )
    };

    // 历史：存储里的 user/assistant 文本
    let mut messages: Vec<Message> = ses::load_messages(&sessions_dir, &session_id)
        .into_iter()
        .filter_map(|m| {
            let text: String = m
                .parts
                .iter()
                .filter_map(|p| match p {
                    ses::Part::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                return None;
            }
            Some(match m.role {
                ses::Role::User => Message::user(text),
                ses::Role::Assistant => Message::assistant(text),
                ses::Role::System => Message::system(text),
            })
        })
        .collect();
    // lead inbox：teammate 来信作为用户角色消息注入（排在本轮新消息之前）
    let inbox = state.team.drain_lead_inbox(&session_id);
    for (from, note) in inbox {
        messages.push(Message::user(format!("[teammate {from}] {note}")));
    }
    if messages.is_empty() {
        messages.push(if with_images { Message::user_with_images(text, images) } else { Message::user(text) });
    }

    // 转录件：run 结束后整条 assistant 消息（文本 + 工具调用）落盘
    let transcript = Arc::new(std::sync::Mutex::new(Vec::<ses::Part>::new()));
    let transcript_writer = transcript.clone();
    let sid = session_id.clone();

    // 取消令牌：注册到 active_runs，run 结束移除（session.abort 可达）
    let cancel = kxen_app::agent::cancel::CancelToken::new();
    kxen_app::core::shared::lock(&state.active_runs).insert(session_id.clone(), cancel.clone());

    let mut ctx = kxen_app::agent::agent_loop::AgentContext {
        registry,
        tracker: kxen_app::tools::fs_tool::FileTracker::default(),
        workdir,
        model,
        store,
        max_turns: 32,
        mrm: Some(state.mrm.read().expect("mrm lock").clone()),
        allowed_tools: None,
        extras: Some(state.extras.clone()),
        hooks: Some(state.hooks.clone()),
        loop_detector: kxen_app::agent::loop_detect::LoopDetector::new(),
        cancel: Some(cancel.clone()),
        team: Some(state.team.clone()),
        team_identity: None,
        session_id: Some(session_id.clone()),
        agents: Some(state.agents.clone()),
        bus: Some(bus.clone()),
        on_event: Arc::new(move |event| {
            use kxen_app::agent::agent_loop::AgentEvent as AE;
            match &event {
                AE::ToolCall { name, summary } => {
                    transcript_writer
                        .lock()
                        .expect("transcript")
                        .push(ses::Part::ToolCall { name: name.clone(), input: json!(summary), output: String::new() });
                }
                AE::ToolResult { name, summary } => {
                    let mut guard = transcript_writer.lock().expect("transcript");
                    if let Some(ses::Part::ToolCall { output, .. }) =
                        guard.iter_mut().rev().find(|p| matches!(p, ses::Part::ToolCall { name: n, output, .. } if n == name && output.is_empty()))
                    {
                        *output = summary.clone();
                    }
                }
                _ => {}
            }
            let mut payload = match serde_json::to_value(&event) {
                Ok(v) => v,
                Err(_) => return,
            };
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("session_id".into(), json!(sid));
            }
            bus.publish(kxen_app::core::event::Event::LlmDelta(payload));
        }),
    };
    let outcome = kxen_app::agent::agent_loop::run_turn(&mut ctx, messages).await;
    kxen_app::core::shared::lock(&state.active_runs).remove(&session_id);
    // 用量累计（状态栏 tokens 段）
    if let Some(stats) = outcome.stats {
        let mut map = kxen_app::core::shared::lock(&state.session_tokens);
        let entry = map.entry(session_id.clone()).or_insert((0, 0));
        entry.0 += stats.input_tokens;
        entry.1 += stats.output_tokens;
    }

    let mut parts = transcript.lock().expect("transcript").clone();
    if !outcome.final_text.is_empty() {
        parts.push(ses::Part::Text { text: outcome.final_text });
    }
    if outcome.aborted {
        parts.push(ses::Part::Text { text: "(已中断)".into() });
    }
    if !parts.is_empty() {
        let assistant_msg = ses::new_message(&session_id, ses::Role::Assistant, parts);
        if let Err(e) = ses::append_message(&sessions_dir, &assistant_msg) {
            tracing::error!(error = %e, "session append failed");
        }
    }
}
