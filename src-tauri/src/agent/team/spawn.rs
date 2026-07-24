// ---------------- spawn / plan 审批 / shutdown ----------------

use crate::agent::cancel::CancelToken;
use crate::core::shared::lock;
use crate::llm::ModelRef;
use std::sync::Arc;
use tokio::sync::Notify;

use super::TeamState;
use super::manager::TeamManager;
use super::member_loop::teammate_loop;
use super::types::{Member, MemberStatus};

impl TeamManager {
    pub(super) fn spawn(
        &self,
        state: &Arc<TeamState>,
        name: String,
        role: String,
        prompt: String,
        model_ref: ModelRef,
        plan_approval: bool,
    ) -> Result<String, String> {
        if lock(&state.members).iter().any(|m| m.name == name) {
            return Err(format!("teammate already exists: {name}"));
        }
        let model_name = model_ref.model.clone();
        lock(&state.members).push(Member {
            name: name.clone(),
            role: role.clone(),
            model: model_ref.clone(),
            status: MemberStatus::Working,
            plan_approval,
            prompt: prompt.clone(),
            approved: !plan_approval,
        });
        self.persist_config(state);
        Self::start_member_loop(state, name, role, prompt, model_ref, !plan_approval);
        Ok(format!("teammate spawned (model {model_name})"))
    }

    /// 成员 loop 启动（spawn 与 restore 共用）：注册活动表 + 重建取消/唤醒通道 + spawn task。
    /// 崩溃重启后 cancels/notifies 是空表，不重建则 shutdown/唤醒对新 loop 全哑。
    pub(super) fn start_member_loop(
        state: &Arc<TeamState>,
        name: String,
        role: String,
        prompt: String,
        model_ref: ModelRef,
        approved: bool,
    ) {
        state.deps.agents.register(&state.session_id, &name, crate::agent::activity::AgentKind::Teammate, &model_ref);
        let cancel = CancelToken::new();
        let notify = Arc::new(Notify::new());
        lock(&state.cancels).insert(name.clone(), cancel.clone());
        lock(&state.notifies).insert(name.clone(), notify.clone());
        // 同步上下文（无 runtime）只能注册通道：spawn 会 panic，restore 场景下次启动再补
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(member = name, "no tokio runtime, member loop deferred");
            return;
        };
        let st = state.clone();
        handle.spawn(async move {
            teammate_loop(st, name, role, model_ref, prompt, approved, cancel, notify).await;
        });
    }

    pub(super) fn plan_verdict(&self, state: &Arc<TeamState>, name: &str, approve: bool, feedback: &str) -> Result<String, String> {
        {
            let mut members = lock(&state.members);
            let Some(member) = members.iter_mut().find(|m| m.name == name) else {
                return Err(format!("teammate not found: {name}"));
            };
            if member.status != MemberStatus::AwaitingPlanApproval {
                return Err(format!("{name} is not awaiting plan approval (status: {:?})", member.status));
            }
            member.status = MemberStatus::Working;
            // 审批结果落盘：崩溃重启后 restore 按 approved 初值续跑，不要求重批
            member.approved = approve;
        }
        self.persist_config(state);
        let text = if approve {
            "[lead] Plan approved. Proceed with implementation.".to_string()
        } else {
            format!("[lead] Plan rejected. Revise and resubmit. Feedback: {feedback}")
        };
        self.send(state, "lead", name, &text)?;
        Ok(if approve { format!("approved {name}") } else { format!("rejected {name} with feedback") })
    }

    pub(super) fn shutdown(&self, state: &Arc<TeamState>, name: &str) -> Result<String, String> {
        let token = lock(&state.cancels).get(name).cloned();
        let Some(token) = token else {
            return Err(format!("teammate not found: {name}"));
        };
        token.cancel();
        if let Some(m) = lock(&state.members).iter_mut().find(|m| m.name == name) {
            m.status = MemberStatus::Shutdown;
        }
        self.persist_config(state);
        Ok(format!("shutdown requested: {name}"))
    }
}
