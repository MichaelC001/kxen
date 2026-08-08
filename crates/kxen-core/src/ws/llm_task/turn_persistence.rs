//! 迭代级持久化装配与崩溃恢复注记（主会话 run 专用；子环境 ctx.persist_turn 为 None）。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use kxen_core::agent::agent_loop::PersistTurn;
use kxen_core::core::session as ses;
use kxen_core::llm::{Message, ModelRef};

/// 迭代持久化回调：本迭代 parts 组成一条 Assistant 消息幂等落盘，
/// message id = `{stream_id}:t{turn}`（stream_id 每 run 唯一，重试/恢复不写双份）。
/// 返回回调与已落盘迭代计数——finalize 的「无输出兜底」据此区分
/// 「迭代已落盘但无最终文本」与「整个 run 无声结束」。
pub(super) fn turn_persister(
    sessions_dir: PathBuf,
    session_id: String,
    stream_id: String,
    model: ModelRef,
) -> (PersistTurn, Arc<AtomicU32>) {
    let persisted = Arc::new(AtomicU32::new(0));
    let counter = persisted.clone();
    let persist: PersistTurn = Arc::new(move |turn, parts| {
        let mut message = ses::new_message(&session_id, ses::Role::Assistant, parts);
        message.id = format!("{stream_id}:t{turn}");
        message.model = Some(model.clone());
        // post-commit 不确定由下层 block_indeterminate 封锁；此处只如实上传失败（fail-closed）
        ses::append_message_idempotent_durable(&sessions_dir, &message).map_err(|error| error.to_string())?;
        counter.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    (persist, persisted)
}

/// 恢复注记：stored_history 末尾是未完结的 turn（最后一条含 ToolCall 的 Assistant 之后
/// 没有最终文本消息）时，在本轮用户消息前补一条注记。崩溃窗口 = 最多一个进行中的迭代，
/// 其副作用不可知，与 cancel 占位同语义（「可能已生效，先核实再重试」）。
pub(super) fn inject_recovery_note(stored_history: &[ses::Message], messages: &mut Vec<Message>) {
    let last_iteration = stored_history
        .iter()
        .rposition(|m| m.role == ses::Role::Assistant && m.parts.iter().any(|p| matches!(p, ses::Part::ToolCall { .. })));
    let last_final = stored_history.iter().rposition(|m| {
        m.role == ses::Role::Assistant
            && m.parts.iter().any(|p| matches!(p, ses::Part::Text { .. }))
            && !m.parts.iter().any(|p| matches!(p, ses::Part::ToolCall { .. }))
    });
    let unfinished = last_iteration.is_some_and(|iteration| last_final.is_none_or(|final_| iteration > final_));
    if !unfinished {
        return;
    }
    // user 角色注入（teammate 来信/compact 摘要同一先例）：部分 provider 不接受序列中段的 system
    let note = Message::user(RECOVERY_NOTE);
    match messages.iter().rposition(|m| m.role == kxen_core::llm::types::Role::User) {
        Some(position) => messages.insert(position, note),
        None => messages.push(note),
    }
}

const RECOVERY_NOTE: &str = "(系统注记：上一个 run 在工具执行阶段可能已中断，最后一个进行中迭代的副作用是否生效未知。继续前请先核实工作区状态，不要盲目重试可能已生效的操作。)";

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("kxen-turn-persist-{tag}-{}-{nanos}", std::process::id()))
    }

    fn iteration(stream: &str, turn: u32, calls: Vec<ses::Part>) -> ses::Message {
        ses::Message {
            id: format!("{stream}:t{turn}"),
            session_id: "ses".into(),
            role: ses::Role::Assistant,
            parts: calls,
            model: None,
            created_at: 0,
        }
    }

    fn call(name: &str, output: &str) -> ses::Part {
        ses::Part::ToolCall { name: name.into(), input: serde_json::json!(name), output: output.into(), args: None, id: None }
    }

    #[test]
    fn iteration_message_id_is_idempotent_under_repeated_append() {
        let dir = temp_dir("idem");
        let meta = ses::create(&dir, "/tmp/work").unwrap();
        let mut message = ses::new_message(&meta.id, ses::Role::Assistant, vec![call("read", "data")]);
        message.id = "run-1-0001:t1".into();

        ses::append_message_idempotent(&dir, &message).unwrap();
        ses::append_message_idempotent(&dir, &message).unwrap();

        let stored = ses::load_messages(&dir, &meta.id);
        assert_eq!(stored.len(), 1, "迭代消息 id 重复写不双份");
        assert_eq!(stored[0].id, "run-1-0001:t1");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn turn_persister_writes_iteration_message_with_stable_id_and_model() {
        let dir = temp_dir("write");
        let meta = ses::create(&dir, "/tmp/work").unwrap();
        let (persist, counter) = turn_persister(dir.clone(), meta.id.clone(), "run-1-0001".into(), ModelRef::new("p", "m"));

        persist(2, vec![call("read", "data")]).unwrap();

        let stored = ses::load_messages(&dir, &meta.id);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, "run-1-0001:t2");
        assert_eq!(stored[0].role, ses::Role::Assistant);
        assert_eq!(stored[0].model.as_ref(), Some(&ModelRef::new("p", "m")));
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn turn_persist_failure_is_returned_not_swallowed() {
        let dir = temp_dir("fail");
        let meta = ses::create(&dir, "/tmp/work").unwrap();
        let (persist, _) = turn_persister(dir.clone(), meta.id.clone(), "run-1-0002".into(), ModelRef::new("p", "m"));
        persist(1, vec![call("read", "a")]).unwrap();
        // 同 id 不同内容：幂等追加按碰撞拒绝，错误必须冒泡给 run loop（fail-closed）
        let error = persist(1, vec![call("read", "different")]).unwrap_err();
        assert!(error.contains("collision"), "{error}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn crash_simulation_recovers_both_iterations_and_injects_note_before_user_message() {
        // 崩溃模拟：第 2 迭代落盘后 run 死亡（无最终文本消息），新一轮用户消息已 commit
        let stored_history = vec![
            iteration("run-1-0001", 1, vec![call("read", "file a"), call("exec", "built")]),
            iteration("run-1-0001", 2, vec![call("write", "written")]),
            ses::new_message("ses", ses::Role::User, vec![ses::Part::Text { text: "继续".into() }]),
        ];
        let mut messages = kxen_core::agent::compact::flatten_stored(&stored_history);

        // flatten 完整含两轮交互：2 × (assistant_with_tools + results)
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[0].tool_calls.len(), 2);
        assert_eq!(messages[3].tool_calls.len(), 1);
        assert_eq!(messages[2].content, "built");
        assert_eq!(messages[4].content, "written");
        assert_eq!(messages[5].role, kxen_core::llm::types::Role::User);

        inject_recovery_note(&stored_history, &mut messages);
        assert_eq!(messages.len(), 7);
        let note = &messages[5];
        assert_eq!(note.role, kxen_core::llm::types::Role::User);
        assert!(note.content.contains("副作用是否生效未知"), "恢复注记必须声明中断窗口");
        assert_eq!(messages[6].content, "继续", "注记插在本轮用户消息之前");
    }

    #[test]
    fn completed_turn_needs_no_recovery_note() {
        let stored_history = vec![
            iteration("run-1-0001", 1, vec![call("read", "file a")]),
            ses::new_message("ses", ses::Role::Assistant, vec![ses::Part::Text { text: "完成".into() }]),
            ses::new_message("ses", ses::Role::User, vec![ses::Part::Text { text: "下一个".into() }]),
        ];
        let mut messages = kxen_core::agent::compact::flatten_stored(&stored_history);
        let before = messages.len();
        inject_recovery_note(&stored_history, &mut messages);
        assert_eq!(messages.len(), before, "正常完结的 turn 不注入恢复注记");
    }

    #[test]
    fn iteration_text_does_not_count_as_final_text() {
        // 迭代消息自身可带 Text part（模型该轮文本）：不能误判为「已有最终文本」
        let mut with_text = iteration("run-1-0001", 1, vec![call("read", "x")]);
        with_text.parts.insert(0, ses::Part::Text { text: "该轮文本".into() });
        let stored_history = vec![with_text, ses::new_message("ses", ses::Role::User, vec![ses::Part::Text { text: "继续".into() }])];
        let mut messages = kxen_core::agent::compact::flatten_stored(&stored_history);
        inject_recovery_note(&stored_history, &mut messages);
        assert!(messages.iter().any(|m| m.content.contains("副作用是否生效未知")));
    }
}
