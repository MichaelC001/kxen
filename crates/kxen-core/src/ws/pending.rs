//! pending queue 的 AppState 侧接线：启动恢复续跑。

use std::sync::Arc;

use crate::AppState;

/// 启动恢复：上次退出前排队的消息逐 session claim 首条续跑，run 收尾 ack 后依次消化剩余。
/// 立即续跑而非等用户再发消息：「已排队」是后端对用户消息的承诺，重启不该变成无限搁置。
pub fn restore_queues(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut ready = state.pending_messages.restore();
        // 中断补投必须在任何新 run 前投递：父 session 本次续跑即见中断事实，主模型不再干等不会来的通知。
        // 顺序上必须先 restore 再 recover：enqueue 基于内存态整写 queue 文件，后于 restore 才不会覆盖
        // 未恢复的存量队列；contains_delivery 副锚也需要内存队列已注水才生效。
        // 补投返回的 sid 定点合并进续跑清单，不再二次全量 restore：restore 对盘上非空队列无条件
        // insert，启动窗口内并发 enqueue（已落盘+更新内存）会被旧盘内容覆盖，消息躺到下次重启；
        // 补投本身已同步更新内存态与盘，返回值即差异全集。
        let recovered = kxen_core::agent::background::recover_interrupted(&state.pending_messages, &kxen_core::core::paths::sessions_dir());
        if !recovered.is_empty() {
            tracing::info!(recovered = recovered.len(), "interrupted background agent notifications recovered");
            for sid in recovered {
                if !ready.contains(&sid) {
                    ready.push(sid);
                }
            }
        }
        if let Some(error) = state.pending_messages.store_error() {
            state.bus.publish(kxen_core::core::event::Event::notify(format!("待处理队列存储不可用，已阻止后续覆盖：{error}"), None));
        }
        for (sid, error) in state.pending_messages.blocked() {
            state
                .bus
                .publish(kxen_core::core::event::Event::notify(format!("会话 {sid} 的待处理队列损坏，已阻止覆盖：{error}"), Some(sid)));
        }
        report_session_recovery(&state);
        for sid in ready {
            // 会话已删（队列文件残留）：清盘不续跑
            match kxen_core::core::session::load_meta(&kxen_core::core::paths::sessions_dir(), &sid) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if let Err(error) = state.pending_messages.clear(&sid) {
                        tracing::warn!(session = sid, %error, "orphan pending queue cleanup failed");
                    }
                    continue;
                }
                Err(error) => {
                    tracing::error!(session = sid, %error, "session metadata unavailable; pending queue preserved");
                    state.bus.publish(kxen_core::core::event::Event::notify(
                        format!("会话元数据不可用，待处理消息已保留：{error}"),
                        Some(sid.clone()),
                    ));
                    continue;
                }
            }
            let (q, cancel) =
                match super::run_slot::claim_queued_run(&state.active_runs, &kxen_core::core::paths::sessions_dir(), &sid, || {
                    state.pending_messages.claim(&sid)
                }) {
                    Ok(Some(claimed)) => claimed,
                    Ok(None) => continue,
                    Err(error) => {
                        tracing::error!(session = sid, %error, "pending queue run claim failed during restore");
                        super::queue_retry::schedule_retry(state.clone(), sid.clone());
                        continue;
                    }
                };
            let stream_id = super::protocol::stream_id("run");
            super::llm_task::spawn_claimed_run(
                super::llm_task::RunInput {
                    stream_id,
                    session_id: sid,
                    text: q.text,
                    context: q.context,
                    images: q.images,
                    queue_delivery_id: Some(q.id),
                    queue_created_at: Some(q.created_at),
                    schedule_job_id: q.schedule_job_id,
                    state: state.clone(),
                },
                cancel,
            );
        }
    });
}

fn report_session_recovery(state: &AppState) {
    let sessions = kxen_core::core::paths::sessions_dir();
    for session in kxen_core::core::session::list(&sessions) {
        let diagnostic = match kxen_core::core::session::inspect_storage(&sessions, &session.id) {
            Ok(report)
                if report.blocked.is_some() || !matches!(&report.messages, kxen_core::core::session::MessageIntegrity::Healthy { .. }) =>
            {
                serde_json::to_string(&report).unwrap_or_else(|_| "storage recovery required".into())
            }
            Ok(_) => continue,
            Err(error) => error,
        };
        tracing::error!(session = session.id, %diagnostic, "session storage recovery required");
        state.bus.publish(kxen_core::core::event::Event::notify(
            format!("会话存储需要恢复检查，已阻止不安全写入：{diagnostic}"),
            Some(session.id),
        ));
    }
}

/// 续跑触发：delivery claim 与 run lease 原子完成；落败 kick 不接触 in_flight。
pub(crate) fn kick_session(state: Arc<AppState>, sid: String) {
    tokio::spawn(async move {
        let (q, cancel) = match super::run_slot::claim_queued_run(&state.active_runs, &kxen_core::core::paths::sessions_dir(), &sid, || {
            state.pending_messages.claim(&sid)
        }) {
            Ok(Some(claimed)) => claimed,
            Ok(None) => {
                let active = kxen_core::core::shared::lock(&state.active_runs).contains_key(&sid);
                if !active && !state.pending_messages.has_queued(&sid) {
                    super::queue_retry::reset_retry(&sid);
                }
                return;
            }
            Err(error) => {
                tracing::error!(session = sid, %error, "pending queue run claim failed");
                super::queue_retry::schedule_retry(state.clone(), sid.clone());
                return;
            }
        };
        let stream_id = super::protocol::stream_id("run");
        super::llm_task::spawn_claimed_run(
            super::llm_task::RunInput {
                stream_id,
                session_id: sid,
                text: q.text,
                context: q.context,
                images: q.images,
                queue_delivery_id: Some(q.id),
                queue_created_at: Some(q.created_at),
                schedule_job_id: q.schedule_job_id,
                state: state.clone(),
            },
            cancel,
        );
    });
}

/// 桥接：relay 的 kick 回调在本层注入（kxen_core agent 层够不着 run_llm 的 spawn 口）
pub fn wire_team_kick(state: &Arc<AppState>) {
    let kick_state = state.clone();
    state.team.relay().set_kick(move |sid| kick_session(kick_state.clone(), sid));
}

/// background late 通知的续跑触发接线：notify.close 的 late 闭包入队后拉活，
/// 与 team kick 共用原子 queue/run admission。
pub fn wire_background_kick(state: &Arc<AppState>) {
    let kick_state = state.clone();
    kxen_core::agent::background::set_late_kick(move |sid| kick_session(kick_state.clone(), sid));
}
