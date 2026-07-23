//! LLM 任务：send_message 触发的 agent run。

use kxen_app::llm::Message;
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::AppState;

pub(crate) async fn run_llm(stream_id: String, session_id: String, text: String, context: Vec<kxen_app::agent::context::ContextItem>, mut images: Vec<kxen_app::llm::types::ImagePart>, app: AppHandle) {
    use kxen_app::core::session as ses;

    let state = app.state::<Arc<AppState>>();
    let sessions_dir = kxen_app::core::paths::sessions_dir();

    // 多 workspace：run 的 workdir 取 session 归属目录（fallback 当前活跃 workspace）
    let session_dir = ses::load_meta(&sessions_dir, &session_id)
        .map(|m| m.directory)
        .unwrap_or_else(|_| state.active_workspace.read().expect("workspace").to_string_lossy().into_owned());
    let session_path = std::path::PathBuf::from(&session_dir);

    // /compact 手动压缩：重写会话历史（蒸馏旧段 + 保留最近），不走正常 run
    if text.trim() == "/compact" {
        let state = app.state::<Arc<AppState>>();
        let model = state.model.lock().map(|m| m.clone()).unwrap_or_default();
        let store = state.auth_store.lock().map(|s| s.clone()).unwrap_or_default();
        let stored = ses::load_messages(&sessions_dir, &session_id);
        let llm_msgs: Vec<kxen_app::llm::Message> = stored
            .iter()
            .filter_map(|m| {
                let text: String = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        ses::Part::Text { text } | ses::Part::Context { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if text.is_empty() {
                    return None;
                }
                Some(match m.role {
                    ses::Role::User => kxen_app::llm::Message::user(text),
                    ses::Role::Assistant => kxen_app::llm::Message::assistant(text),
                    ses::Role::System => kxen_app::llm::Message::system(text),
                })
            })
            .collect();
        let before = kxen_app::agent::compact::estimate_tokens(&llm_msgs);
        let compacted = kxen_app::agent::compact::compact_messages(&model, &store, &llm_msgs, 4).await;
        let after = kxen_app::agent::compact::estimate_tokens(&compacted);
        // 回写：每条压缩后消息转 stored（text part），图片不保留（压缩的既定代价）
        let stored_msgs: Vec<ses::Message> = compacted
            .iter()
            .map(|m| {
                let role = match m.role {
                    kxen_app::llm::types::Role::User => ses::Role::User,
                    _ => ses::Role::Assistant,
                };
                ses::new_message(&session_id, role, vec![ses::Part::Text { text: m.content.clone() }])
            })
            .collect();
        if let Err(e) = ses::rewrite_messages(&sessions_dir, &session_id, &stored_msgs) {
            tracing::error!(error = %e, "compact rewrite failed");
        }
        let notice = format!("上下文已压缩：约 {before} -> {after} tokens");
        let msg = ses::new_message(&session_id, ses::Role::Assistant, vec![ses::Part::Text { text: notice }]);
        let _ = ses::append_message(&sessions_dir, &msg);
        // done 事件让前端收敛（不发 run，前端在等终态）
        state.bus.publish(kxen_app::core::event::Event::LlmDelta(serde_json::json!({
            "kind": "done", "session_id": session_id, "stream_id": stream_id,
        })));
        return;
    }

    // 自定义 / 命令展开：kind=Command 条目 $ARGUMENTS 模板 + needs 依赖懒加载（builtin 由模型 playbook 处理）
    let text = if let Some(rest) = text.strip_prefix('/') {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("");
        let args = parts.next().unwrap_or("").trim();
        kxen_app::agent::commands::expand(&session_path, name, args).unwrap_or(text)
    } else {
        text
    };

    // @ 引用注入：chip -> 上下文块（文件/目录/Web/Docs），追加在用户消息尾部。
    // 图片 URL 分流：content-type 判定为图片的直挂 images 通道（公网图片输入），其余走文本注入。
    let (context_block, context_failures) = {
        let mut text_items = Vec::new();
        for item in context {
            let is_image = match &item {
                kxen_app::agent::context::ContextItem::Web { url }
                | kxen_app::agent::context::ContextItem::Docs { url } => {
                    if let Some(img) = kxen_app::agent::context::fetch_image_url(url).await {
                        images.push(img);
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if !is_image {
                text_items.push(item);
            }
        }
        if text_items.is_empty() {
            (String::new(), Vec::new())
        } else {
            kxen_app::agent::context::build_context(&text_items, &session_path).await
        }
    };
    for f in &context_failures {
        state.bus.publish(kxen_app::core::event::Event::Notification(format!("引用读取失败：{f}")));
    }

    // 用户消息落盘：展示文本与注入上下文分 part 存（UI 只显示 Text，模型历史两者皆见）
    let mut parts = vec![ses::Part::Text { text: text.clone() }];
    if !context_block.is_empty() {
        parts.push(ses::Part::Context { text: context_block.clone() });
    }
    let user_msg = ses::new_message(&session_id, ses::Role::User, parts);
    let with_images = !images.is_empty();
    if let Err(e) = ses::append_message(&sessions_dir, &user_msg) {
        tracing::error!(error = %e, "session append failed");
        return;
    }
    // checkpoint：turn 前状态打 shadow git 检查点（后台异步，不阻塞 run 启动）
    {
        let dir = session_path.clone();
        let label = user_msg.id.clone();
        tokio::spawn(async move {
            match tokio::task::spawn_blocking(move || kxen_app::tools::checkpoint::commit(&dir, &label)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(error = %e, "checkpoint commit failed"),
                Err(e) => tracing::warn!(error = %e, "checkpoint commit join failed"),
            }
        });
    }
    let text = if context_block.is_empty() { text } else { format!("{text}\n{context_block}") };

    let (model, store, registry, workdir, bus) = {
        // 主会话模型快过期先刷新（克隆出来刷避免持锁跨 await；成功则回写共享 store）
        let provider = state.model.lock().map(|m| m.provider.clone()).unwrap_or_default();
        let account = state.model.lock().ok().and_then(|m| m.account.clone());
        let mut store = state.auth_store.lock().map(|s| s.clone()).unwrap_or_default();
        let refreshed = kxen_app::auth::refresh::ensure_fresh(&mut store, &provider, account.as_deref()).await;
        if refreshed {
            let key = account.as_deref().map(|a| kxen_app::auth::credential::account_id(&provider, a)).unwrap_or(provider.clone());
            if let Some(cred) = store.get(&key).cloned() {
                state.auth_store.lock().expect("auth_store").insert(key, cred);
            }
        }
        (
            state.model.lock().map(|m| m.clone()).unwrap_or_default(),
            store,
            state.registry.clone(),
            std::sync::Arc::from(session_path.as_path()),
            state.bus.clone(),
        )
    };

    // 历史：存储里的 user/assistant 文本（Context part 同样喂给模型，UI 侧不显示）
    let mut messages: Vec<Message> = ses::load_messages(&sessions_dir, &session_id)
        .into_iter()
        .filter_map(|m| {
            let text: String = m
                .parts
                .iter()
                .filter_map(|p| match p {
                    ses::Part::Text { text } | ses::Part::Context { text } => Some(text.as_str()),
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
    // 图片挂到当前用户消息（刚落盘为纯文本）：原位替换，历史其余不变
    if with_images {
        match messages.iter().rposition(|m| m.role == kxen_app::llm::types::Role::User && m.content == text) {
            Some(pos) => messages[pos] = Message::user_with_images(text, images),
            None => messages.push(Message::user_with_images(text, images)),
        }
    } else if messages.is_empty() {
        messages.push(Message::user(text));
    }

    // 转录件：run 结束后整条 assistant 消息（文本 + 工具调用）落盘
    let transcript = Arc::new(std::sync::Mutex::new(Vec::<ses::Part>::new()));
    let transcript_writer = transcript.clone();
    let sid = session_id.clone();
    let stream_id_event = stream_id.clone();

    // 取消令牌：注册到 active_runs，run 结束移除（session.abort 可达）
    let cancel = kxen_app::agent::cancel::CancelToken::new();
    kxen_app::core::shared::lock(&state.active_runs).insert(session_id.clone(), cancel.clone());

    let mut ctx = kxen_app::agent::agent_loop::AgentContext {
        registry,
        tracker: {
            let mut t = kxen_app::tools::fs_tool::FileTracker::default();
            // 会话级改动快照：改动面板「本会话 agent 改了什么」的数据源
            t.snapshots = kxen_app::core::shared::lock(&state.session_snapshots)
                .entry(session_id.clone())
                .or_default()
                .clone();
            t
        },
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
        approvals: Some(state.approvals.clone()),
        mcp: Some(state.mcp.clone()),
        lsp: Some(state.lsp.read().expect("lsp").clone()),
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
                obj.insert("stream_id".into(), json!(stream_id_event));
            }
            bus.publish(kxen_app::core::event::Event::LlmDelta(payload));
        }),
    };
    let outcome = kxen_app::agent::agent_loop::run_turn(&mut ctx, messages).await;
    kxen_app::core::shared::lock(&state.session_involved).insert(session_id.clone(), ctx.tracker.files());
    kxen_app::core::shared::lock(&state.active_runs).remove(&session_id);
    // stop hook（run 结束挂点，fire-and-log）
    if let Err(e) = state
        .hooks
        .run_named("stop", &session_id, &serde_json::json!({ "session_id": session_id, "aborted": outcome.aborted }))
        .await
    {
        tracing::warn!(error = %e, "stop hook failed");
    }
    kxen_app::core::shared::lock(&state.run_streams).remove(&stream_id);
    // 用量累计（状态栏 tokens 段）
    if let Some(stats) = outcome.stats {
        let mut map = kxen_app::core::shared::lock(&state.session_tokens);
        let entry = map.entry(session_id.clone()).or_insert((0, 0));
        entry.0 += stats.input_tokens;
        entry.1 += stats.output_tokens;
        drop(map);
        kxen_app::core::shared::lock(&state.session_last_input).insert(session_id.clone(), stats.input_tokens);
    }

    let mut parts = transcript.lock().expect("transcript").clone();
    if !outcome.final_text.is_empty() {
        parts.push(ses::Part::Text { text: outcome.final_text });
    }
    if outcome.aborted {
        parts.push(ses::Part::Text { text: "(已中断)".into() });
    }
    // 兜底：任何路径都不许无声结束（会话只剩用户消息是 P0 事故）
    if parts.is_empty() {
        parts.push(ses::Part::Text { text: "(run 异常结束，无输出——请重试或发送「继续」)".into() });
    }
    let assistant_msg = ses::new_message(&session_id, ses::Role::Assistant, parts);
    if let Err(e) = ses::append_message(&sessions_dir, &assistant_msg) {
        tracing::error!(error = %e, "session append failed");
    }

    // 队列下一条：run 进行中收到的消息按序接续（后端锁是唯一竞态防线，前端状态不可靠）
    let next = kxen_app::core::shared::lock(&state.pending_messages)
        .get_mut(&session_id)
        .and_then(|q| q.pop_front());
    if let Some((text, context, images)) = next {
        let stream_id = super::protocol::stream_id("run");
        kxen_app::core::shared::lock(&state.run_streams).insert(stream_id.clone(), session_id.clone());
        spawn_run(stream_id, session_id, text, context, images, app.clone());
    }
}

/// 队列续跑的 spawn 断路器：在 run_llm 体内直接 spawn 自身会让 future 类型递归自嵌套（E0283），
/// 经普通 fn 间接一层后类型层面不再自引用。
fn spawn_run(
    stream_id: String,
    session_id: String,
    text: String,
    context: Vec<kxen_app::agent::context::ContextItem>,
    images: Vec<kxen_app::llm::types::ImagePart>,
    app: AppHandle,
) {
    tokio::spawn(run_llm(stream_id, session_id, text, context, images, app));
}
