//! 子代理活动的落盘布局与重启恢复扫描（AgentRegistry 的磁盘半；内存 ring 语义在 activity.rs）。
//!
//! 布局（agents_root = sessions_dir，AppState 启动注入）：
//!   转录（UI 事件写穿）  <agents_root>/<sid>/agents/<name>.transcript.jsonl
//!   turn 历史（LLM 消息） <agents_root>/<sid>/agents/<name>.turns.jsonl
//! 放会话目录旁：subagent 属主是会话，session remove 连同会话子目录一起清理；
//! teammate 转录仍走 team_root（teams/<sid>/transcripts/），两者不混。

use crate::agent::activity::{ActivityStatus, TRANSCRIPT_CAP};
use std::path::{Path, PathBuf};

/// 转录落盘路径：teammate -> team_root（既有布局），subagent -> agents_root。
/// 无对应 root（测试/无持久化上下文）或非法 id 返回 None = 纯内存。
pub(crate) fn transcript_path(
    team_root: Option<&Path>,
    agents_root: Option<&Path>,
    kind: crate::agent::activity::AgentKind,
    session_id: &str,
    name: &str,
) -> Option<PathBuf> {
    use crate::agent::activity::AgentKind;
    if crate::core::ids::validate_id(session_id).is_err() || crate::core::ids::validate_id(name).is_err() {
        tracing::warn!(session_id, name, "transcript persist skipped: invalid id");
        return None;
    }
    match kind {
        AgentKind::Teammate => team_root.map(|root| root.join(session_id).join("transcripts").join(format!("{name}.jsonl"))),
        AgentKind::Subagent => agents_root.map(|root| agents_dir(root, session_id).join(format!("{name}.transcript.jsonl"))),
        // workflow 有自己的 journal 持久化，不双写
        AgentKind::Workflow => None,
    }
}

/// turn 级历史路径（subagent persist_turn 落点；恢复真源/审计，subagent 一次性不续跑）。
pub(crate) fn run_log_path(agents_root: Option<&Path>, session_id: &str, name: &str) -> Option<PathBuf> {
    if crate::core::ids::validate_id(session_id).is_err() || crate::core::ids::validate_id(name).is_err() {
        return None;
    }
    agents_root.map(|root| agents_dir(root, session_id).join(format!("{name}.turns.jsonl")))
}

fn agents_dir(root: &Path, session_id: &str) -> PathBuf {
    root.join(session_id).join("agents")
}

/// 转录重放（teammate register 时注水）：坏行跳过（观测面不 fail-closed）。
pub(crate) fn rehydrate(path: Option<PathBuf>) -> std::collections::VecDeque<serde_json::Value> {
    let mut out = std::collections::VecDeque::new();
    let Some(path) = path else { return out };
    let Ok(text) = std::fs::read_to_string(path) else { return out };
    for line in text.lines() {
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) => {
                if out.len() >= TRANSCRIPT_CAP {
                    out.pop_front();
                }
                out.push_back(value);
            }
            Err(error) => tracing::warn!(%error, "dropping malformed transcript line"),
        }
    }
    out
}

/// 追加写一行 JSONL；落盘失败只告警不丢内存态（transcript 是观测面，不该拖死 agent loop）
pub(crate) fn append_line(path: Option<PathBuf>, payload: &serde_json::Value) {
    use std::io::Write;
    let Some(path) = path else { return };
    let Some(parent) = path.parent() else { return };
    let Ok(line) = serde_json::to_string(payload) else { return };
    let result = std::fs::create_dir_all(parent).and_then(|()| {
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{line}")
    });
    if let Err(error) = result {
        tracing::warn!(%error, "transcript persist failed");
    }
}

/// 从盘恢复的 subagent 条目注入 registry（惰性恢复，activity.rs ensure_restored_locked 调用）；
/// 已在内存的同名条目不覆盖。model 署名为 restored（重启后实际路由模型不可考）。
pub(crate) fn restore_into(entries: &mut Vec<crate::agent::activity::AgentActivity>, agents_root: &Path, session_id: &str) {
    for restored in scan_session(agents_root, session_id) {
        if entries.iter().any(|a| a.name == restored.name) {
            continue;
        }
        entries.push(crate::agent::activity::AgentActivity {
            name: restored.name,
            kind: crate::agent::activity::AgentKind::Subagent,
            model: crate::llm::ModelRef::new("restored", "restored"),
            status: restored.status,
            started_at: restored.started_at,
            transcript: restored.transcript,
        });
    }
}

/// 从盘恢复的 subagent 记录（重启后 registry 重建条目用）。
pub(crate) struct RestoredAgent {
    pub(crate) name: String,
    pub(crate) status: ActivityStatus,
    pub(crate) started_at: u64,
    pub(crate) transcript: std::collections::VecDeque<serde_json::Value>,
}

/// 扫描 <sid>/agents/*.transcript.jsonl 重建条目。转录是观测面：坏行跳过不 fail-closed
///（与 teammate rehydrate 同口径）。status 语义：转录含 done 事件 = run 在进程死前完结 -> Done；
/// 否则进程中断 -> Shutdown（与 agents.stop 主动停同语义，UI 已有渲染）。
pub(crate) fn scan_session(agents_root: &Path, session_id: &str) -> Vec<RestoredAgent> {
    let mut out = Vec::new();
    if crate::core::ids::validate_id(session_id).is_err() {
        return out;
    }
    let entries = match std::fs::read_dir(agents_dir(agents_root, session_id)) {
        Ok(entries) => entries,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let file = entry.file_name();
        let Some(name) = file.to_str().and_then(|file| file.strip_suffix(".transcript.jsonl")) else { continue };
        if crate::core::ids::validate_id(name).is_err() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
        let mut transcript = std::collections::VecDeque::new();
        let mut done = false;
        for line in text.lines() {
            match serde_json::from_str::<serde_json::Value>(line) {
                Ok(value) => {
                    done |= value.get("kind").and_then(|kind| kind.as_str()) == Some("done");
                    if transcript.len() >= TRANSCRIPT_CAP {
                        transcript.pop_front();
                    }
                    transcript.push_back(value);
                }
                Err(error) => tracing::warn!(%error, "dropping malformed transcript line"),
            }
        }
        // 转录无时间戳：起始时间取文件 mtime，仅供 UI 排序参考
        let started_at = entry
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        out.push(RestoredAgent {
            name: name.to_string(),
            status: if done { ActivityStatus::Done } else { ActivityStatus::Shutdown },
            started_at,
            transcript,
        });
    }
    out
}

#[cfg(test)]
mod tests;
