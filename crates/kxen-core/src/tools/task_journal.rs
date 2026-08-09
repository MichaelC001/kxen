//! exec/task 后台进程的任务日志（DCP）：start/exit/killed/reaped 追加进 session 的
//! tasks.jsonl（append-only），进程重启后由 agent/background/task_recovery.rs 按
//! 「每个 task_id 的最后一行」判定中断：start = 中断待补投，exit/killed/reaped = 已收口。
//! 落盘走与 session/kanban 相同的 storage::append_synced 基建，不发明第二套追加/sync 口径。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// start 行的 command 截断上限：命令可能内嵌整段脚本，日志只需可辨识前缀。
const COMMAND_CAP: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskLine {
    Start {
        task_id: String,
        command: String,
        workdir: String,
        pid: Option<u32>,
        started_at: u64,
    },
    Exit {
        task_id: String,
        exit_code: i32,
        ended_at: u64,
    },
    Killed {
        task_id: String,
        ended_at: u64,
    },
    /// 恢复已处置的幂等锚：投递成功后落本行，下次启动该任务不再是 start 收尾，自然跳过。
    Reaped {
        task_id: String,
        ended_at: u64,
    },
}

impl TaskLine {
    pub fn start(task_id: &str, command: &str, workdir: &str, pid: Option<u32>) -> Self {
        Self::Start {
            task_id: task_id.to_string(),
            command: command.chars().take(COMMAND_CAP).collect(),
            workdir: workdir.to_string(),
            pid,
            started_at: crate::core::shared::now_ms(),
        }
    }

    pub fn exit(task_id: &str, exit_code: i32) -> Self {
        Self::Exit { task_id: task_id.to_string(), exit_code, ended_at: crate::core::shared::now_ms() }
    }

    pub fn killed(task_id: &str) -> Self {
        Self::Killed { task_id: task_id.to_string(), ended_at: crate::core::shared::now_ms() }
    }

    pub fn reaped(task_id: &str) -> Self {
        Self::Reaped { task_id: task_id.to_string(), ended_at: crate::core::shared::now_ms() }
    }

    fn task_id(&self) -> &str {
        match self {
            Self::Start { task_id, .. } | Self::Exit { task_id, .. } | Self::Killed { task_id, .. } | Self::Reaped { task_id, .. } => {
                task_id
            }
        }
    }
}

/// 日志路径。owner id 过不了 id 校验（kanban 的 exec_scope 形如 `kanban:<run_id>` 含冒号）
/// 就不落盘：该上下文回执明说 "no notification in this context"，无承诺即无洞，跳过是设计内。
fn path_for(sessions_dir: &Path, session_id: &str) -> Option<PathBuf> {
    crate::core::ids::validate_id(session_id).ok()?;
    Some(sessions_dir.join(session_id).join("tasks.jsonl"))
}

/// 追加一行。写失败只 warn 不阻断调用方：后台进程本身是主效果，日志 IO 不该拖死
/// spawn/执行/终止；降级方向有意 fail-open（回到无日志的旧行为），恢复侧按缺失处理。
pub(crate) fn append(sessions_dir: Option<&Path>, session_id: &str, line: &TaskLine) {
    let Some(sessions_dir) = sessions_dir else { return };
    let Some(path) = path_for(sessions_dir, session_id) else { return };
    if let Err(error) = append_inner(&path, line) {
        tracing::warn!(%error, task = line.task_id(), "task journal append failed");
    }
}

fn append_inner(path: &Path, line: &TaskLine) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec(line).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    crate::core::session::storage::append_synced(path, &bytes).map_err(|failure| failure.to_string())
}

/// 恢复扫描读出的中断任务（最后一行是 start）。
#[derive(Debug)]
pub struct OpenTask {
    pub task_id: String,
    pub command: String,
    pub pid: Option<u32>,
}

/// 读 tasks.jsonl：每个 task_id 只保留最后一行，最后一行为 start 的即中断任务。
/// 坏行跳过而非整本失败：崩溃可能撕断最后半行，严格失败会把同 session 其它任务的
/// 恢复一起拖死；漏读的最坏结果是该任务不被补投（fail-open，与写侧同方向）。
pub fn read_open_tasks(path: &Path) -> Vec<OpenTask> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "task journal read failed");
            return Vec::new();
        }
    };
    let mut last: HashMap<String, TaskLine> = HashMap::new();
    for raw in text.lines() {
        match serde_json::from_str::<TaskLine>(raw) {
            Ok(line) => {
                last.insert(line.task_id().to_string(), line);
            }
            Err(error) => tracing::warn!(%error, path = %path.display(), "task journal line skipped"),
        }
    }
    last.into_values()
        .filter_map(|line| match line {
            TaskLine::Start { task_id, command, pid, .. } => Some(OpenTask { task_id, command, pid }),
            TaskLine::Exit { .. } | TaskLine::Killed { .. } | TaskLine::Reaped { .. } => None,
        })
        .collect()
}
