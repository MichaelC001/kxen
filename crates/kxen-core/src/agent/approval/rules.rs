//! 审批规则短路（B1）：「本会话放行」「总是放行」建规后的匹配/落表/撤销。
//! fail-closed 顺序不变：Deny 判定永远先于规则表（safety_gate 内先评估后查表）；
//! 命中先写 durable 审计（decision=rule_allow）再放行，审计失败即不自动放行。

use super::ApprovalBroker;
use crate::agent::approval_rules::{self, ApprovalRule, RuleScope};

impl ApprovalBroker {
    /// 建规入口（ApprovalCard「本会话放行/总是放行」）：命令全文 trim 后作为前缀。
    /// session 规则纯内存随进程失效；workspace 规则写 <workspace>/.agents/kxen/approval-rules.json。
    pub fn remember_rule(&self, session_id: &str, command: &str, reason: &str, scope: RuleScope) -> Result<ApprovalRule, String> {
        let prefix = approval_rules::validate_prefix(command)?;
        let rule = ApprovalRule {
            id: crate::core::ids::new_id("rule"),
            prefix,
            scope,
            session_id: match scope {
                RuleScope::Session => Some(session_id.to_string()),
                RuleScope::Workspace => None,
            },
            created_at_ms: crate::core::shared::now_ms(),
            expires_at_ms: None,
            max_uses: None,
            used: 0,
            reason: reason.to_string(),
        };
        match scope {
            RuleScope::Session => {
                if session_id.is_empty() {
                    return Err("无会话归属的审批不能建会话规则".into());
                }
                crate::core::shared::lock(&self.session_rules).push(rule.clone());
            }
            RuleScope::Workspace => {
                let workspace = self.workspace_for_session(session_id).ok_or("会话 workspace 不可解析，不能建 workspace 规则")?;
                let mut rules = self.cached_workspace_rules(&workspace)?;
                rules.push(rule.clone());
                approval_rules::save_workspace_rules(&workspace, &rules)?;
                crate::core::shared::lock(&self.workspace_rules).insert(workspace, rules);
            }
        }
        Ok(rule)
    }

    /// 规则表短路（safety_gate 在 Deny 之后、逐次审批之前调用）：
    /// 命中先写 durable 审计（decision=rule_allow）再放行；审计/计数落盘失败返回 Err，
    /// 调用方回落逐次审批——绝不「放了但没记」。
    pub fn try_rule_allow(&self, session_id: &str, command: &str, reason: &str) -> Result<(), String> {
        use approval_rules::{alive, matches};
        if session_id.is_empty() {
            return Err("无会话归属，规则表不适用".into());
        }
        let now = crate::core::shared::now_ms();
        // session 规则：惰性清理失效项后找首个命中
        {
            let mut rules = crate::core::shared::lock(&self.session_rules);
            rules.retain(|rule| alive(rule, now));
            if let Some(rule) = rules.iter_mut().find(|rule| rule.session_id.as_deref() == Some(session_id) && matches(rule, command)) {
                self.persist_decision_checked(session_id, command, reason, "rule_allow")?;
                rule.used += 1;
                return Ok(());
            }
        }
        let Some(workspace) = self.workspace_for_session(session_id) else {
            return Err("会话 workspace 不可解析".into());
        };
        let mut rules = self.cached_workspace_rules(&workspace)?;
        rules.retain(|rule| alive(rule, now));
        let Some(pos) = rules.iter().position(|rule| matches(rule, command)) else {
            return Err("no matching approval rule".into());
        };
        self.persist_decision_checked(session_id, command, reason, "rule_allow")?;
        rules[pos].used += 1;
        // 计数（max_uses 消耗）必须随规则落盘，否则重启即重置额度
        approval_rules::save_workspace_rules(&workspace, &rules)?;
        crate::core::shared::lock(&self.workspace_rules).insert(workspace, rules);
        Ok(())
    }

    /// 规则列表（approval_rules.list RPC）：session 规则 + 该 workspace 规则，失效项惰性清理不展示。
    pub fn list_rules(&self, session_id: Option<&str>, workspace: &std::path::Path) -> Vec<ApprovalRule> {
        let now = crate::core::shared::now_ms();
        let mut out = Vec::new();
        {
            let mut rules = crate::core::shared::lock(&self.session_rules);
            rules.retain(|rule| approval_rules::alive(rule, now));
            out.extend(rules.iter().filter(|rule| session_id.is_none() || rule.session_id.as_deref() == session_id).cloned());
        }
        if let Ok(rules) = self.cached_workspace_rules(workspace) {
            out.extend(rules.into_iter().filter(|rule| approval_rules::alive(rule, now)));
        }
        out
    }

    /// 撤销规则（approval_rules.revoke RPC）：session 内存摘除 / workspace 文件摘除并落盘。
    pub fn revoke_rule(&self, id: &str, workspace: &std::path::Path) -> Result<bool, String> {
        {
            let mut rules = crate::core::shared::lock(&self.session_rules);
            let before = rules.len();
            rules.retain(|rule| rule.id != id);
            if rules.len() != before {
                return Ok(true);
            }
        }
        let mut rules = self.cached_workspace_rules(workspace)?;
        let before = rules.len();
        rules.retain(|rule| rule.id != id);
        if rules.len() == before {
            return Ok(false);
        }
        approval_rules::save_workspace_rules(workspace, &rules)?;
        crate::core::shared::lock(&self.workspace_rules).insert(workspace.to_path_buf(), rules);
        Ok(true)
    }

    /// 会话 -> workspace 解析（规则归属与 workspace 规则文件定位共用）。
    fn workspace_for_session(&self, session_id: &str) -> Option<std::path::PathBuf> {
        let dir = self.sessions_dir.as_ref()?;
        let meta = crate::core::session::load_meta(dir, session_id).ok()?;
        Some(std::path::PathBuf::from(meta.directory))
    }

    fn cached_workspace_rules(&self, workspace: &std::path::Path) -> Result<Vec<ApprovalRule>, String> {
        if let Some(rules) = crate::core::shared::lock(&self.workspace_rules).get(workspace) {
            return Ok(rules.clone());
        }
        let rules = approval_rules::load_workspace_rules(workspace)?;
        crate::core::shared::lock(&self.workspace_rules).insert(workspace.to_path_buf(), rules.clone());
        Ok(rules)
    }
}
