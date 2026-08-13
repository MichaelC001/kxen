//! exec/task 后台进程的中断补投：进程死在 watcher 送达前时，启动据 tasks.jsonl
//! 「每个 task_id 最后一行 = start」判定中断，尽力回收残留进程并把「结果未知」事实
//! 投递给 owning session（与 recover_interrupted 同语义，返回实际投递到的 session id）。
//! WHY 存活即 kill：kxen 已死过一轮，该进程的 stdout 管道断裂、输出无人收割，
//! 留着是无主泄漏；SIGTERM 即发即弃，失败只 warn。
//! WHY 扫不到 kanban scope：kanban run 的 owner（`kanban:<run_id>`）过不了 id 校验，
//! spawn 时就不落日志（tools/task_journal.rs），这里天然没有它的行。

use crate::agent::background::{RoutedNotice, deliver_late, kick_late, recoverable_session_ids};
use crate::core::pending_queue::PendingQueues;
use crate::tools::task_journal::{self, TaskLine};
use std::path::Path;

/// 通知里的 command 截断：通知是单行事实陈述，完整命令在 tasks.jsonl。
const NOTICE_COMMAND_CAP: usize = 120;
/// pid 合理性上限（2^22 = Linux pid_max 默认值）：日志行可能损坏，超范围的 pid 不做任何信号操作。
#[cfg(unix)]
const MAX_REASONABLE_PID: u32 = 4_194_304;

/// 遍历 sessions_dir 下每个 session 的 tasks.jsonl，为最后一行是 start 的任务补投中断通知。
/// 幂等双锚：reaped 行（主）挡重复启动；确定性 delivery id 在队检查（副）挡队列未消费的重投。
pub fn recover_interrupted_tasks(pending: &PendingQueues, sessions_dir: &Path) -> Vec<String> {
    let mut delivered = Vec::new();
    let session_ids = match recoverable_session_ids(sessions_dir) {
        Ok(session_ids) => session_ids,
        Err(error) => {
            tracing::warn!(%error, "interrupted background task recovery scan failed");
            return delivered;
        }
    };
    for sid in session_ids {
        for task in task_journal::read_open_tasks(&sessions_dir.join(&sid).join("tasks.jsonl")) {
            reap_task(pending, sessions_dir, &sid, &task, &mut delivered);
        }
    }
    delivered
}

fn reap_task(pending: &PendingQueues, sessions_dir: &Path, sid: &str, task: &task_journal::OpenTask, delivered: &mut Vec<String>) {
    sigterm_if_alive(task.pid, &task.task_id);
    let delivery_id = format!("bgtask-{}", task.task_id);
    // contains_delivery 副锚：上轮投递仍在队列时跳过（不补 reaped 行，与 agent 恢复同取舍：
    // 残留重复投递窗口概率低且同 id 重放入队被拒，结果无害）
    if pending.contains_delivery(sid, &delivery_id) {
        return;
    }
    let command: String = task.command.chars().take(NOTICE_COMMAND_CAP).collect();
    let notice = RoutedNotice {
        id: delivery_id,
        text: format!(
            "[task notification] background task {} ({command}) was interrupted by a process restart; \
             its outcome is unknown - restart it explicitly if still needed.",
            task.task_id
        ),
        created_at: crate::core::shared::now_ms(),
        persisted: false,
    };
    match deliver_late(pending, sessions_dir, sid, notice) {
        Ok(_) => {
            // reaped 幂等锚只在投递成功后落：投递失败不落，下次启动重试
            task_journal::append(Some(sessions_dir), sid, &TaskLine::reaped(&task.task_id));
            // 进程在跑时立即拉活续跑；未接线（测试）时通知躺队列由既有兜底，不丢
            kick_late(sid);
            delivered.push(sid.to_string());
        }
        Err(error) => {
            tracing::warn!(%error, session = sid, task = task.task_id, "interrupted background task notification delivery failed");
        }
    }
}

/// unix：kill -0 探测存活则对进程组 SIGTERM——spawn 时 process_group(0) 使 pgid == pid，
/// 与 TaskRegistry::terminate 同法覆盖 shell 的孙进程。
/// 非 unix 无进程组语义与外部 kill 命令：跳过探测与信号，只投递中断事实。
#[cfg(unix)]
fn sigterm_if_alive(pid: Option<u32>, task_id: &str) {
    let kill_quiet = |args: &[&str]| {
        std::process::Command::new("kill").args(args).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
    };
    let Some(pid) = pid.filter(|pid| *pid > 0 && *pid <= MAX_REASONABLE_PID) else { return };
    if kill_quiet(&["-0", &pid.to_string()]) && !kill_quiet(&["-TERM", "--", &format!("-{pid}")]) {
        tracing::warn!(task = task_id, pid, "orphaned background task SIGTERM failed");
    }
}

#[cfg(not(unix))]
fn sigterm_if_alive(_pid: Option<u32>, _task_id: &str) {}
