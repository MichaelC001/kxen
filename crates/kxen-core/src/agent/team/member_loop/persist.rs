// ---------------- teammate turn 级历史（per-member JSONL） ----------------
// 布局：<team dir>/history/<member>.jsonl，行为 session Message（与主会话 JSONL 同 Part 形态，
// flatten_stored 同口径重建）。放 team 目录而非 sessions 目录：member 不是 session（无 meta），
// 生命周期随 team 目录（drop_session 整目录清理），与 config/tasks/inboxes/transcripts 同位置。
//
// message id 确定性（成员 + wake 维度，崩溃重放/重试幂等）：
//   {name}:w{wake}:u       首轮 brief / nudge 的 user 注入
//   {name}:in:{delivery}   inbox 来信驱动的 user 注入（delivery id 崩溃重放不变，免双份）
//   {name}:w{wake}:t{turn} run 内迭代（persist_turn，build_ctx 装配）
//   {name}:w{wake}:final   wake 末轮 assistant 文本（run 收尾落盘，缺档会让恢复丢掉本轮结论）

use crate::core::session as ses;
use crate::llm::ModelRef;
use std::path::{Path, PathBuf};

use super::super::inbox::InboxDelivery;

pub(super) fn path(dir: &Path, name: &str) -> PathBuf {
    dir.join("history").join(format!("{name}.jsonl"))
}

/// 严格读取（torn/坏行 fail-closed）：按降级历史起跑会让模型基于残缺上下文行动。
pub(super) fn load(dir: &Path, name: &str) -> Result<Vec<ses::Message>, String> {
    crate::core::ids::validate_id(name)?;
    ses::load_lines(&path(dir, name)).map_err(|error| format!("read member history {name}: {error}"))
}

/// 下一 wake 序号：恢复后从盘续号，新 wake 不复用旧 id（幂等门禁不挡新内容）。
pub(super) fn next_wake(stored: &[ses::Message]) -> u32 {
    stored.iter().filter_map(|message| wake_of(&message.id)).max().unwrap_or(0).saturating_add(1).max(1)
}

fn wake_of(id: &str) -> Option<u32> {
    let start = id.find(":w")? + 2;
    let end = id[start..].find(':').map(|offset| start + offset)?;
    id[start..end].parse().ok()
}

/// 恢复时 prompt 是否就是落盘的首条 brief：restart_members 原样重启（同 -> 不重复注入），
/// resume_member 换成 recovery_prompt（不同 -> 作为新指令注入）。首条 user 消息以 brief 原文开头
///（first_prompt 输出 = brief 前缀 + inbox/claims 后缀），starts_with 判定足够。
pub(super) fn is_original_brief(stored: &[ses::Message], prompt: &str) -> bool {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return true;
    }
    stored.iter().find(|message| message.role == ses::Role::User).and_then(first_text).is_some_and(|text| text.starts_with(prompt))
}

fn first_text(message: &ses::Message) -> Option<&str> {
    message.parts.iter().find_map(|part| match part {
        ses::Part::Text { text } => Some(text.as_str()),
        _ => None,
    })
}

/// 启动恢复：读盘 -> flatten 重建 -> 恢复注记（只进内存不落盘，主会话同口径）。
/// 返回 (stored, history, next_wake)；stored 随回（is_original_brief 判定用）。
pub(super) fn restore(dir: &Path, name: &str) -> Result<(Vec<ses::Message>, Vec<crate::llm::Message>, u32), String> {
    let stored = load(dir, name)?;
    let wake = next_wake(&stored);
    let mut history = crate::agent::compact::flatten_stored(&stored);
    if !stored.is_empty() {
        history.push(crate::llm::Message::user(recovery_note(&stored)));
    }
    Ok((stored, history, wake))
}

/// user 注入的 message id：inbox 驱动用首条 entry 的 delivery id（未 ack 崩溃重放同 id 幂等），
/// brief/nudge 用 wake 序号。
pub(super) fn user_message_id(name: &str, wake: u32, delivery: Option<&InboxDelivery>) -> String {
    match delivery.and_then(|delivery| delivery.entries.first()) {
        Some(entry) => format!("{name}:in:{}", entry.transcript_id),
        None => format!("{name}:w{wake}:u"),
    }
}

pub(super) fn append_user(dir: &Path, name: &str, session_id: &str, id: &str, text: &str) -> Result<(), String> {
    let mut message = ses::new_message(session_id, ses::Role::User, vec![ses::Part::Text { text: text.into() }]);
    message.id = id.into();
    append(dir, name, &message)
}

pub(super) fn append_final(dir: &Path, name: &str, session_id: &str, wake: u32, model: &ModelRef, text: &str) -> Result<(), String> {
    let mut message = ses::new_message(session_id, ses::Role::Assistant, vec![ses::Part::Text { text: text.into() }]);
    message.id = format!("{name}:w{wake}:final");
    message.model = Some(model.clone());
    append(dir, name, &message)
}

fn append(dir: &Path, name: &str, message: &ses::Message) -> Result<(), String> {
    // post-commit 不确定由调用方按 block_member 处理（fail-closed，与主会话同口径）
    ses::append_line_idempotent(&path(dir, name), message).map_err(|error| error.to_string())
}

/// 恢复注记（只进内存不落盘，主会话 inject_recovery_note 同口径）：
/// 末尾是未完结迭代 -> 崩溃窗口声明；正常收尾 -> 重启续跑声明（兼作本轮驱动的 user 消息）。
pub(super) fn recovery_note(stored: &[ses::Message]) -> &'static str {
    if unfinished(stored) { CRASH_NOTE } else { RESTART_NOTE }
}

/// 与主会话 inject_recovery_note 同判定：最后一个含 ToolCall 的 Assistant 之后没有最终文本消息。
fn unfinished(stored: &[ses::Message]) -> bool {
    let last_iteration =
        stored.iter().rposition(|m| m.role == ses::Role::Assistant && m.parts.iter().any(|p| matches!(p, ses::Part::ToolCall { .. })));
    let last_final = stored.iter().rposition(|m| {
        m.role == ses::Role::Assistant
            && m.parts.iter().any(|p| matches!(p, ses::Part::Text { .. }))
            && !m.parts.iter().any(|p| matches!(p, ses::Part::ToolCall { .. }))
    });
    last_iteration.is_some_and(|iteration| last_final.is_none_or(|final_| iteration > final_))
}

const CRASH_NOTE: &str = "(系统注记：进程已重启，你的历史从磁盘恢复。上一个 run 在工具执行阶段可能已中断，最后一个进行中迭代的副作用是否生效未知。继续前请先核实工作区状态，不要盲目重试可能已生效的操作。)";
const RESTART_NOTE: &str = "(系统注记：进程已重启，你的历史从磁盘恢复。请从恢复点继续未完成的工作；来信与任务状态以本轮输入为准。)";

#[cfg(test)]
mod tests;
