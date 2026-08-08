//! 后台子代理中断补投：进程死在子代理完结前时，启动把「中断事实」投递给父 session。
//!
//! WHY Shutdown 语义 = 进程死在子代理完结前，子代理结果不可知：按「中断置 Unknown」对齐
//! goal completion 口径，绝不猜结果，只投递「结果未送达」这一事实，是否重派由主模型决定。
//! WHY 转录含 done 的条目不投递：通知大概率已持久化；done 到 notify 之间的崩溃残留窗口是
//! 有意不覆盖的取舍——为极小窗口引入结果重放会破坏「绝不猜结果」的口径。
//! WHY 不考虑 kanban scope：notify 为 None 时 background 派发入口直接报错，
//! kanban run 的后台派发不可能走到这条路径。

use crate::agent::activity::ActivityStatus;
use crate::agent::background::{RoutedNotice, deliver_late, kick_late};
use crate::core::pending_queue::PendingQueues;
use std::path::{Path, PathBuf};

/// 遍历 sessions_dir 下每个 session，为 Shutdown 子代理补投中断通知，返回实际投递条数（启动日志用）。
/// 幂等双锚：marker 文件（主）挡重复启动；确定性 delivery id 在队检查（副）挡队列未消费的重投。
pub fn recover_interrupted(pending: &PendingQueues, sessions_dir: &Path) -> usize {
    let mut delivered = 0;
    let entries = match std::fs::read_dir(sessions_dir) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, "interrupted background agent recovery scan failed");
            return 0;
        }
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(sid) = entry.file_name().to_str().map(str::to_owned) else { continue };
        // 目录名即 session id：非法名（路径穿越/空格等）跳过不 panic
        if crate::core::ids::validate_id(&sid).is_err() {
            continue;
        }
        delivered += recover_session(pending, sessions_dir, &sid);
    }
    delivered
}

fn recover_session(pending: &PendingQueues, sessions_dir: &Path, sid: &str) -> usize {
    let mut delivered = 0;
    for agent in crate::agent::activity_disk::scan_session(sessions_dir, sid) {
        if agent.status != ActivityStatus::Shutdown {
            continue;
        }
        let marker = marker_path(sessions_dir, sid, &agent.name);
        if marker.exists() {
            continue;
        }
        let delivery_id = format!("bgshutdown-{}", agent.name);
        if pending.contains_delivery(sid, &delivery_id) {
            continue;
        }
        let notice = RoutedNotice {
            id: delivery_id,
            text: format!(
                "[task notification] agent {} was interrupted by a process restart before its result was delivered; \
                 the outcome is unknown - re-dispatch it explicitly if the work is still needed.",
                agent.name
            ),
            created_at: crate::core::shared::now_ms(),
            persisted: false,
        };
        match deliver_late(pending, sessions_dir, sid, notice) {
            Ok(_) => {
                // marker 写失败只 warn 不翻转：contains_delivery 与 session JSONL 幂等 append 是第二道，
                // 残留重复投递窗口概率低且结果无害（同 id 重放入队被拒/落盘去重），注释即取舍记录
                if let Err(error) = std::fs::write(&marker, crate::core::shared::now_ms().to_string()) {
                    tracing::warn!(%error, session = sid, agent = agent.name, "shutdown marker persist failed");
                }
                // 进程在跑时立即拉活续跑；未接线（测试）时通知躺队列由既有兜底，不丢
                kick_late(sid);
                delivered += 1;
            }
            Err(error) => {
                // 投递失败不写 marker：下次启动重试
                tracing::warn!(%error, session = sid, agent = agent.name, "interrupted background agent notification delivery failed");
            }
        }
    }
    delivered
}

fn marker_path(sessions_dir: &Path, sid: &str, name: &str) -> PathBuf {
    sessions_dir.join(sid).join("agents").join(format!("{name}.shutdown-notified"))
}
