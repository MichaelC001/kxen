use super::*;
use crate::agent::compact::flatten_stored;
use crate::llm::types::Role as LlmRole;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-member-history-{tag}-{}-{nanos}", std::process::id()))
}

fn call(name: &str, output: &str) -> ses::Part {
    ses::Part::ToolCall { name: name.into(), input: serde_json::json!(name), output: output.into(), args: None, id: None }
}

fn iteration(dir: &Path, name: &str, wake: u32, turn: u32, parts: Vec<ses::Part>) {
    let mut message = ses::new_message("s1", ses::Role::Assistant, parts);
    message.id = format!("{name}:w{wake}:t{turn}");
    append(dir, name, &message).unwrap();
}

/// 崩溃重启模拟：wake1 完整（brief + 迭代 + final），wake2 有 inbox 来信与一条迭代后死亡。
/// 从盘重建必须含 brief、工具交互、final 与 inbox 来信（来信不丢），且 wire 合法。
#[test]
fn rebuild_after_crash_restores_full_history_including_inbox() {
    let dir = temp("rebuild");
    let name = "w";
    append_user(&dir, name, "s1", "w:w1:u", "brief: build X").unwrap();
    iteration(&dir, name, 1, 1, vec![call("read", "file content")]);
    append_final(&dir, name, "s1", 1, &ModelRef::new("p", "m"), "done reading").unwrap();
    append_user(&dir, name, "s1", "w:in:msg_1", "[lead] continue").unwrap();
    iteration(&dir, name, 2, 1, vec![call("exec", "built")]);

    let stored = load(&dir, name).unwrap();
    assert_eq!(next_wake(&stored), 3);
    let messages = flatten_stored(&stored);
    assert!(messages.iter().any(|m| m.content == "brief: build X"), "brief 必须保留");
    assert!(messages.iter().any(|m| m.content == "file content"), "工具结果必须重建");
    assert!(messages.iter().any(|m| m.content == "done reading"), "final 必须保留");
    assert!(messages.iter().any(|m| m.role == LlmRole::User && m.content == "[lead] continue"), "inbox 来信不得丢失");
    assert!(messages.iter().any(|m| m.content == "built"), "崩溃前已落盘迭代必须保留");
    // wire 合法：每个 tool call 有配对 result
    for (index, message) in messages.iter().enumerate() {
        for call in &message.tool_calls {
            assert!(
                messages[index + 1..].iter().any(|m| m.tool_call_id.as_deref() == Some(call.id.as_str())),
                "tool call 必须有配对 result: {}",
                call.id
            );
        }
    }
    // 末尾是未完结迭代 -> 崩溃窗口注记
    assert_eq!(recovery_note(&stored), CRASH_NOTE);
    std::fs::remove_dir_all(dir).ok();
}

/// 正常收尾（末尾 final）恢复 -> 续跑注记；幂等重写不双份
#[test]
fn completed_wake_gets_restart_note_and_rewrite_stays_single() {
    let dir = temp("restart");
    let name = "w";
    append_user(&dir, name, "s1", "w:w1:u", "brief").unwrap();
    iteration(&dir, name, 1, 1, vec![call("read", "data")]);
    append_final(&dir, name, "s1", 1, &ModelRef::new("p", "m"), "done").unwrap();
    append_final(&dir, name, "s1", 1, &ModelRef::new("p", "m"), "done").unwrap();

    let stored = load(&dir, name).unwrap();
    assert_eq!(stored.len(), 3, "幂等重写不双份");
    assert_eq!(recovery_note(&stored), RESTART_NOTE);
    assert!(is_original_brief(&stored, "brief"), "原样重启不得重复注入 brief");
    assert!(!is_original_brief(&stored, "new recovery instruction"), "新指令必须注入");
    std::fs::remove_dir_all(dir).ok();
}

/// inbox 驱动的 user 消息用 delivery id：崩溃重放同 id 幂等；同 id 不同内容拒绝
#[test]
fn inbox_driven_user_message_is_replay_idempotent() {
    let dir = temp("inbox-id");
    let name = "w";
    append_user(&dir, name, "s1", "w:in:msg_9", "[lead] ping").unwrap();
    append_user(&dir, name, "s1", "w:in:msg_9", "[lead] ping").unwrap();
    assert!(append_user(&dir, name, "s1", "w:in:msg_9", "[lead] changed").is_err());
    assert_eq!(load(&dir, name).unwrap().len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn wake_numbers_parse_across_two_digits() {
    let dir = temp("wake10");
    let name = "w";
    for wake in [1, 2, 10] {
        append_user(&dir, name, "s1", &format!("w:w{wake}:u"), "hi").unwrap();
    }
    let stored = load(&dir, name).unwrap();
    assert_eq!(next_wake(&stored), 11);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn invalid_member_name_rejected() {
    let dir = temp("bad-name");
    assert!(load(&dir, "../escape").is_err());
    std::fs::remove_dir_all(dir).ok();
}
