// ---------------- spawn / plan 审批 / shutdown ----------------

use crate::agent::cancel::CancelToken;
use crate::core::shared::lock;
use crate::llm::ModelRef;
use std::sync::Arc;
use tokio::sync::Notify;

use super::manager::TeamManager;
use super::member_loop::teammate_loop;
use super::types::{Member, MemberStatus};
use super::TeamState;

impl TeamManager {
    pub(super) fn spawn(&self, state: &Arc<TeamState>, name: String, role: String, prompt: String, model_ref: ModelRef, plan_approval: bool) -> Result<String, String> {
        if lock(&state.members).iter().any(|m| m.name == name) {
            return Err(format!("teammate already exists: {name}"));
        }
        state.deps.agents.register(&state.session_id, &name, crate::agent::activity::AgentKind::Teammate, &model_ref);
        let cancel = CancelToken::new();
        let notify = Arc::new(Notify::new());
        lock(&state.cancels).insert(name.clone(), cancel.clone());
        lock(&state.notifies).insert(name.clone(), notify.clone());
        lock(&state.members).push(Member { name: name.clone(), role: role.clone(), model: model_ref.clone(), status: MemberStatus::Working, plan_approval });
        self.persist_config(state);

        let st = state.clone();
        let (n, r, m, p, pa, c, nt) = (name, role, model_ref.clone(), prompt, plan_approval, cancel, notify);
        tokio::spawn(async move {
            teammate_loop(st, n, r, m, p, pa, c, nt).await;
        });
        Ok(format!("teammate spawned (model {})", model_ref.model))
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
