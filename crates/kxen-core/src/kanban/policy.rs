//! 看板级自主授权（P3）的 AutoApprove 实现：per-run 句柄，exec 的 safety_gate 命中即放行。
//! 放行与审计的原子点 = Board::apply(AutoApproved)：board_lock 串行化，守卫（授权存活/时限/
//! 次数/前缀/run open）全过才追加 auto_approved 事件；审计先 durable 再返回 Ok，绝不「放了但没记」。
//! 守卫拒绝原样转 Err：不自动放行不等于拒绝执行，safety_gate 回落逐次审批。
//! 审计 = kanban 事件流：kanban run 的 session_id 为 None，Part::Approval 会话落盘不适用；
//! 命令本体已在 turns JSONL 的工具调用记录里。

use std::path::PathBuf;

use super::command::Board;
use super::events::KanbanCommand;

pub struct BoardAutoApprove {
    pub workspace: PathBuf,
    pub board_id: String,
    pub run_id: String,
    pub bus: crate::core::event::EventBus,
}

impl crate::tools::auto_approve::AutoApprove for BoardAutoApprove {
    fn try_auto_allow(&self, command: &str) -> Result<(), String> {
        let mut board = Board::open(&self.workspace, &self.board_id).map_err(|error| error.to_string())?;
        board
            .apply(KanbanCommand::AutoApproved { run_id: self.run_id.clone(), command: command.to_string() })
            .map_err(|error| error.to_string())?;
        // 审计走事件流（durable 真源）；广播只做板粒度失效通知让看板页重拉 policy 徽标。
        // 命令原文不进全局流：LlmDelta 无 ACL 全局广播，原文下发即泄漏面。广播失败不算错误
        self.bus.publish(crate::core::event::Event::KanbanUpdate {
            board_id: self.board_id.clone(),
            workspace: self.workspace.to_string_lossy().into_owned(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
