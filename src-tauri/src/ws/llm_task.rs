//! LLM 任务：send_message 触发的 agent run。

use kxen_app::llm::Message;
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::AppState;

pub(crate) async fn run_llm(
    stream_id: String,
    session_id: String,
    text: String,
    context: Vec<kxen_app::agent::context::ContextItem>,
    mut images: Vec<kxen_app::llm::types::ImagePart>,
    app: AppHandle,
) {
    use kxen_app::core::session as ses;

    let state = app.state::<Arc<AppState>>();
    let sessions_dir = kxen_app::core::paths::sessions_dir();

    // cron 触发的 run：消息前缀 [cron <id>]（main.rs tick 注入格式），run 结束回写 job 执行历史
    let cron_job_id = text.strip_prefix("[cron ").and_then(|rest| rest.split(']').next()).map(str::to_string);

    // 多 workspace：run 的 workdir 取 session 归属目录（fallback 当前活跃 workspace）
    let session_dir = ses::load_meta(&sessions_dir, &session_id)
        .map(|m| m.directory)
        .unwrap_or_else(|_| state.active_workspace.read().expect("workspace").to_string_lossy().into_owned());
    let session_path = std::path::PathBuf::from(&session_dir);

    // /compact 手动压缩：蒸馏旧段落检查点（原始 JSONL 不动，rewind 锚点保留），不走正常 run
    if text.trim() == "/compact" {
        let model = super::session_ops::effective_session_model(Some(&session_id), &state);
        let store = state.auth_store.lock().map(|s| s.clone()).unwrap_or_default();
        let notice = match kxen_app::agent::compact::compact_session(&sessions_dir, &session_id, &model, &store, 4).await {
            Some((before, after)) => format!("上下文已压缩：约 {before} -> {after} tokens"),
            None => "历史太短，无需压缩".to_string(),
        };
        let msg = ses::new_message(&session_id, ses::Role::Assistant, vec![ses::Part::Text { text: notice }]);
        let _ = ses::append_message(&sessions_dir, &msg);
        // done 事件让前端收敛（不发 run，前端在等终态）
        state.bus.publish(kxen_app::core::event::Event::LlmDelta(serde_json::json!({
            "kind": "done", "session_id": session_id, "stream_id": stream_id,
        })));
        return;
    }

    // /doctor 环境自检：doctor 报告直出（落盘 + done 事件），不走 LLM
    if crate::doctor::is_doctor_command(&text) {
        crate::doctor::reply_with_report(&state, &sessions_dir, &session_id, &stream_id).await;
        return;
    }

    // run 期守卫：持到本函数结束——rewind 写锁全程被挡（原子性），存亡广播驱动侧栏 running 圆点（core::rewind_lock）
    let _run_guard = kxen_app::core::rewind_lock::run_guard(&session_dir, &session_id, &state.bus).await;
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
                kxen_app::agent::context::ContextItem::Web { url } | kxen_app::agent::context::ContextItem::Docs { url } => {
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
            // picked 授权快照随 run 固定：run 中途新增授权不进本轮注入
            let picked = state.picked_files.snapshot(&session_id);
            kxen_app::agent::context::build_context(&text_items, &session_path, picked.as_ref()).await
        }
    };
    for f in &context_failures {
        state.bus.publish(kxen_app::core::event::Event::notify(format!("引用读取失败：{f}"), Some(session_id.clone())));
    }

    // 用户消息落盘：展示文本与注入上下文分 part 存（UI 只显示 Text，模型历史两者皆见）
    let mut parts = vec![ses::Part::Text { text: text.clone() }];
    if !context_block.is_empty() {
        parts.push(ses::Part::Context { text: context_block.clone() });
    }
    // 图片逐个落 Part::Image（base64 内联）：重开/导出/fork 均可见，会话目录自包含
    for img in &images {
        parts.push(ses::Part::Image { media_type: img.media_type.clone(), data: img.data.clone() });
    }
    let user_msg = ses::new_message(&session_id, ses::Role::User, parts);
    let with_images = !images.is_empty();
    if let Err(e) = ses::append_message(&sessions_dir, &user_msg) {
        tracing::error!(error = %e, "session append failed");
        return;
    }
    // checkpoint 屏障：turn 前状态打 shadow git 检查点，等落盘完成再进 run
    // （rewind 依赖该 commit 存在；失败只 warn 不阻塞 run）
    kxen_app::tools::checkpoint::checkpoint_barrier(&session_path, &user_msg.id).await;
    let text = if context_block.is_empty() { text } else { format!("{text}\n{context_block}") };

    let (model, store, registry, workdir, bus) = {
        // 主会话模型快过期先刷新（克隆出来刷避免持锁跨 await；成功则回写共享 store）
        let model = super::session_ops::effective_session_model(Some(&session_id), &state);
        let provider = model.provider.clone();
        let account = model.account.clone();
        let mut store = state.auth_store.lock().map(|s| s.clone()).unwrap_or_default();
        let refreshed = kxen_app::auth::refresh::ensure_fresh(&mut store, &provider, account.as_deref()).await;
        if refreshed {
            let key = account.as_deref().map(|a| kxen_app::auth::credential::account_id(&provider, a)).unwrap_or(provider.clone());
            if let Some(cred) = store.get(&key).cloned() {
                state.auth_store.lock().expect("auth_store").insert(key, cred);
            }
        }
        (model, store, state.registry.clone(), std::sync::Arc::from(session_path.as_path()), state.bus.clone())
    };

    // 历史：应用压缩检查点后的模型视图（Text/Context 进模型，其余 part 丢弃；与 compact 同口径）
    let mut messages: Vec<Message> = kxen_app::agent::compact::flatten_stored(&ses::load_history(&sessions_dir, &session_id));
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

    // 转录件：run 结束后整条 assistant 消息（reasoning + 工具调用 + 文本）一次落盘
    let transcript = Arc::new(std::sync::Mutex::new(Vec::<ses::Part>::new()));
    let transcript_writer = transcript.clone();
    let sid = session_id.clone();
    let stream_id_event = stream_id.clone();
    let sessions_dir_event = sessions_dir.clone();

    // 取消令牌：注册到 active_runs，run 结束移除（session.abort 可达）
    let cancel = kxen_app::agent::cancel::CancelToken::new();
    kxen_app::core::shared::lock(&state.active_runs).insert(session_id.clone(), cancel.clone());

    // 后台 agent 完成通知路由：run 存活期由 run loop 逐轮 drain 注入 messages；
    // run 收尾 close 后（含 run 结束后才完成的派发）通知直投 pending queue，由队列续跑消化
    let notify = std::sync::Arc::new(kxen_app::agent::background::NotifyRouter::new());
    // P0-2a：注册给 team relay，teammate -> lead 报告经本 run 的 router 就地注入（run 收尾摘除）
    state.team.relay().register(&session_id, &notify);

    let mut ctx = kxen_app::agent::agent_loop::AgentContext {
        registry,
        tracker: {
            let mut t = kxen_app::tools::fs_tool::FileTracker::default();
            // 会话级改动快照：改动面板「本会话 agent 改了什么」的数据源
            t.snapshots = kxen_app::core::shared::lock(&state.session_snapshots).entry(session_id.clone()).or_default().clone();
            t
        },
        workdir,
        model,
        store,
        max_turns: 32,
        mrm: Some(state.mrm.read().expect("mrm lock").clone()),
        allowed_tools: None,
        extras: Some(state.extras_for(&session_id)),
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
        notify: Some(notify.clone()),
        on_event: Arc::new(move |event| {
            use kxen_app::agent::agent_loop::AgentEvent as AE;
            match &event {
                AE::Reasoning { text } => {
                    // 分片落盘为整块：连续 reasoning delta 并入尾部 Reasoning part
                    let mut guard = transcript_writer.lock().expect("transcript");
                    match guard.last_mut() {
                        Some(ses::Part::Reasoning { text: existing }) => existing.push_str(text),
                        _ => guard.push(ses::Part::Reasoning { text: text.clone() }),
                    }
                }
                AE::ToolCall { name, summary, arguments } => {
                    // input 留一行摘要（UI 头行），args 存精确参数；parse 失败留原文不丢数据
                    let args = serde_json::from_str(arguments).unwrap_or_else(|_| json!(arguments));
                    transcript_writer.lock().expect("transcript").push(ses::Part::ToolCall {
                        name: name.clone(),
                        input: json!(summary),
                        output: String::new(),
                        args: Some(args),
                    });
                }
                AE::ToolResult { name, output, .. } => {
                    let mut guard = transcript_writer.lock().expect("transcript");
                    if let Some(ses::Part::ToolCall { output: slot, .. }) = guard
                        .iter_mut()
                        .rev()
                        .find(|p| matches!(p, ses::Part::ToolCall { name: n, output, .. } if n == name && output.is_empty()))
                    {
                        // 完整结果落盘，cap 10_000 字节防 JSONL 单行爆炸（UI 折叠区本就截断展示）
                        *slot = cap_output(output, 10_000);
                    }
                }
                AE::Compacted { summary } => {
                    // auto-compact 落检查点（upto = 当前存储尾消息 id）；前端无对应渲染，不上行
                    if let Some(upto) = ses::load_messages(&sessions_dir_event, &sid).last().map(|m| m.id.clone()) {
                        let _ = ses::save_compaction(&sessions_dir_event, &sid, &ses::Compaction::new(upto, summary.clone()));
                    }
                    return;
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
    let outcome = kxen_app::agent::agent_loop::run_turn(&mut ctx, &mut messages).await;
    // 通知路由收尾：通道残留与此后到达的通知全部入队 + kick 拉活（kick_session 判活，无活跃 run 才起）。
    // 本 run 的收尾 pop（下方）立即消化残留，kick 撞见活跃 run / 空队列即退，不并发起第二个 run。
    notify.close({
        let state = state.inner().clone();
        let sid = session_id.clone();
        std::sync::Arc::new(move |text: String| {
            state.pending_messages.enqueue(&sid, text, vec![], vec![]);
            kxen_app::agent::background::kick_late(&sid);
        })
    });
    // P0-2a 摘除：此后 teammate -> lead 报告走 pending queue 续跑路（relay 查无 router）
    state.team.relay().unregister(&session_id, &notify);
    kxen_app::core::shared::lock(&state.session_involved).insert(session_id.clone(), ctx.tracker.files());
    // 代际匹配才摘 token：interrupt 策略下新 run 已占位，无条件 remove 会删掉新 run 的 abort 通道
    kxen_app::agent::cancel::remove_if_current(&mut kxen_app::core::shared::lock(&state.active_runs), &session_id, &cancel);
    // run 收尾清掉本 session 挂起的审批：等待方按 deny 唤醒，防 pending 泄漏（session 删除同理可达）
    state.approvals.cancel_session(&session_id);
    // stop hook（run 结束挂点，fire-and-log；Ask 档走审批通道）
    let stop_appr =
        kxen_app::tools::exec::ApprovalCtx::new(Some(state.approvals.as_ref()), Some(&state.bus), Some(&cancel), Some(session_id.as_str()));
    if let Err(e) = state
        .hooks
        .run_named_with_approval(
            "stop",
            &session_id,
            &serde_json::json!({ "session_id": session_id, "aborted": outcome.aborted }),
            stop_appr.as_ref(),
        )
        .await
    {
        tracing::warn!(error = %e, "stop hook failed");
    }
    kxen_app::core::shared::lock(&state.run_streams).remove(&stream_id);
    // 用量累计（状态栏 tokens 段；落盘供重启恢复）
    if let Some(stats) = outcome.stats {
        let mut map = kxen_app::core::shared::lock(&state.session_tokens);
        let entry = map.entry(session_id.clone()).or_insert((0, 0));
        entry.0 += stats.input_tokens;
        entry.1 += stats.output_tokens;
        kxen_app::core::usage::persist(&map);
        drop(map);
        // ctx 水位取最近一次请求的 input（累计值不代表窗口占用）
        kxen_app::core::shared::lock(&state.session_last_input).insert(session_id.clone(), stats.last_input_tokens);
    }

    // cron 执行历史回写（schedule.list 的最近执行状态；job 已删则 record 静默丢弃）
    if let Some(job_id) = cron_job_id {
        let errored = outcome.final_text.starts_with("(错误");
        let error = if outcome.aborted {
            Some("run 被中断".to_string())
        } else if errored {
            Some(outcome.final_text.chars().take(200).collect())
        } else {
            None
        };
        kxen_app::core::schedule::record(&job_id, !outcome.aborted && !errored, error);
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

    // 队列下一条：run 进行中收到的消息按序接续（pop 即落盘重写，崩溃窗口丢一条与旧纯内存等价）。
    // pop 前复查本 run token：已 cancel（abort/interrupt）不续跑——收尾 pop 起新 run 会让 abort 失效；
    // 残留队列由下一次 send 的 run 收尾或重启 restore 消化
    let next = if cancel.is_cancelled() { None } else { state.pending_messages.pop(&session_id) };
    if let Some(q) = next {
        let stream_id = super::protocol::stream_id("run");
        kxen_app::core::shared::lock(&state.run_streams).insert(stream_id.clone(), session_id.clone());
        spawn_run(stream_id, session_id, q.text, q.context, q.images, app.clone());
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

/// 转录落盘的单行上限：截在 char 边界上（多字节字符不截烂）
fn cap_output(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { s[..s.floor_char_boundary(max)].to_string() }
}
