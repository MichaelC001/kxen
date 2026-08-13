//! 后台子代理中断补投：进程死在子代理完结前时，启动把「中断事实」投递给父 session。
//!
//! WHY 只补投转录无终态事件的条目（进程真正死在完结前）：子代理结果不可知，按「中断置 Unknown」
//! 对齐 goal completion 口径，绝不猜结果，只投递「结果未送达」这一事实，是否重派由主模型决定。
//! 三档终态均不补投：done = 完结通知路径在旧进程已走过；aborted = 显式停止（agents.stop /
//! 父 run abort 级联），发起方已知，投「结果未知请重派」会诱导重派已明确取消的工作；
//! error = 失败通知在旧进程已 best-effort 送达，崩溃截断失败通知的残留窗口是有意接受的
//!（转录与 UI 仍可见失败终态），不复述成 unknown 制造与既有通知的矛盾。
//! WHY 不考虑 kanban scope：notify 为 None 时 background 派发入口直接报错，
//! kanban run 的后台派发不可能走到这条路径。

use crate::agent::background::{RoutedNotice, deliver_late, kick_late, recoverable_session_ids};
use crate::core::pending_queue::PendingQueues;
use std::path::{Path, PathBuf};

/// 遍历 sessions_dir 下每个 session，为无终态的中断子代理补投中断通知，
/// 返回实际投递到的 session id 列表（调用方合并续跑清单与启动日志用）。
/// 幂等双锚：marker 文件（主）挡重复启动；确定性 delivery id 在队检查（副）挡队列未消费的重投。
pub fn recover_interrupted(pending: &PendingQueues, sessions_dir: &Path) -> Vec<String> {
    let mut delivered = Vec::new();
    let session_ids = match recoverable_session_ids(sessions_dir) {
        Ok(session_ids) => session_ids,
        Err(error) => {
            tracing::warn!(%error, "interrupted background agent recovery scan failed");
            return delivered;
        }
    };
    for sid in session_ids {
        delivered.extend(recover_session(pending, sessions_dir, &sid));
    }
    delivered
}

fn recover_session(pending: &PendingQueues, sessions_dir: &Path, sid: &str) -> Vec<String> {
    let mut delivered = Vec::new();
    for agent in crate::agent::activity_disk::scan_session(sessions_dir, sid) {
        // 终态三档（done/aborted/error）的通知路径在旧进程都已走过，见模块头注释；只补投无终态
        if agent.terminal.is_some() {
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
                delivered.push(sid.to_string());
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
