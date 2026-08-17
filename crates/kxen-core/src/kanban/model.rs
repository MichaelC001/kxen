//! 列模型与看板元数据（对齐 design.md「列模型」YAML 语义：on_enter / transitions / wip_limit）。

use serde::{Deserialize, Serialize};

use super::error::KanbanError;
use super::events::Outcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnEnterKind {
    #[default]
    None,
    AgentRun,
    Workflow,
    HumanGate,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OnEnter {
    pub kind: OnEnterKind,
    pub agent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Transitions {
    pub on_success: Option<String>,
    pub on_failure: Option<String>,
}

impl Transitions {
    /// Timeout 不对应迁移：超时只把卡片停成 blocked，由人/重试决定下一步。
    pub fn target(&self, outcome: Outcome) -> Option<&str> {
        match outcome {
            Outcome::Success => self.on_success.as_deref(),
            Outcome::Failure => self.on_failure.as_deref(),
            Outcome::Timeout => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnDef {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub on_enter: OnEnter,
    #[serde(default)]
    pub transitions: Transitions,
    #[serde(default)]
    pub wip_limit: Option<u32>,
    /// 列执行超时（毫秒，P2a）；None = driver 默认值（driver.rs DEFAULT_RUN_TIMEOUT_MS，含 WHY）。
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl ColumnDef {
    pub fn validate(&self) -> Result<(), KanbanError> {
        crate::core::ids::validate_id(&self.id).map_err(KanbanError::InvalidId)?;
        if self.title.trim().is_empty() {
            return Err(KanbanError::InvalidColumn("column title is required".into()));
        }
        match self.on_enter.kind {
            OnEnterKind::AgentRun | OnEnterKind::Workflow if self.on_enter.agent.as_deref().is_none_or(str::is_empty) => {
                return Err(KanbanError::InvalidColumn(format!(
                    "column {} kind {:?} requires agent (definition file in .agents/kxen/kanban/agents/)",
                    self.id, self.on_enter.kind
                )));
            }
            OnEnterKind::None | OnEnterKind::HumanGate if self.on_enter.agent.is_some() => {
                return Err(KanbanError::InvalidColumn(format!(
                    "column {} kind {:?} must not reference agent",
                    self.id, self.on_enter.kind
                )));
            }
            _ => {}
        }
        // wip_limit = 0 等于永久锁死该列，只能是误配
        if self.wip_limit == Some(0) {
            return Err(KanbanError::InvalidColumn(format!("column {} wip_limit must be >= 1", self.id)));
        }
        // timeout_ms = 0 等于每次执行立即超时，只能是误配
        if self.timeout_ms == Some(0) {
            return Err(KanbanError::InvalidColumn(format!("column {} timeout_ms must be >= 1", self.id)));
        }
        Ok(())
    }
}

/// 建板/加列的共享校验：列自身合法 + id 唯一 + transitions 目标都在列集合内（不存在的目标即误配）。
pub fn validate_columns(columns: &[ColumnDef]) -> Result<(), KanbanError> {
    for column in columns {
        column.validate()?;
        if columns.iter().filter(|c| c.id == column.id).count() > 1 {
            return Err(KanbanError::ColumnExists(column.id.clone()));
        }
        for target in [&column.transitions.on_success, &column.transitions.on_failure].into_iter().flatten() {
            if !columns.iter().any(|c| &c.id == target) {
                return Err(KanbanError::ColumnNotFound(target.clone()));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    Ready,
    WaitingHuman,
    Running,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardComment {
    pub author: String,
    pub body: String,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardState {
    pub id: String,
    pub column_id: String,
    pub title: String,
    pub body: String,
    pub status: CardStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    #[serde(default)]
    pub comments: Vec<CardComment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunState {
    pub id: String,
    pub card_id: String,
    pub column_id: String,
    pub attempt: u32,
    pub started_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
}

/// 看板级自主授权配置（P3）：命令前缀 allowlist + 可选时限 + 可选最大自动放行次数。
/// 只能由人经核心 API（KanbanCommand::PolicySet）设置，不暴露为 Agent 工具——模型能自我扩权 = 安全漏洞。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicySpec {
    pub allowlist: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDef {
    pub name: String,
    pub role: String,
    pub model: String,
    pub permission_profile: String,
    /// custom profile 的工具白名单（固定三档恒 None）；旧快照无此字段，serde default 兼容加载。
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    pub defined_at: u64,
}

/// 默认软件开发模板（design.md）：前两列人工门控对齐「意图锁定区」，完成列无出边为终态。
pub fn default_template() -> Vec<ColumnDef> {
    let column =
        |id: &str, title: &str, kind: OnEnterKind, agent: Option<&str>, on_success: Option<&str>, on_failure: Option<&str>| ColumnDef {
            id: id.into(),
            title: title.into(),
            on_enter: OnEnter { kind, agent: agent.map(str::to_string) },
            transitions: Transitions { on_success: on_success.map(str::to_string), on_failure: on_failure.map(str::to_string) },
            wip_limit: None,
            timeout_ms: None,
        };
    vec![
        column("requirements", "需求", OnEnterKind::HumanGate, None, Some("implementing"), None),
        column("implementing", "实现中", OnEnterKind::AgentRun, Some("execution"), Some("testing"), Some("requirements")),
        column("testing", "测试中", OnEnterKind::AgentRun, Some("qa"), Some("review"), Some("implementing")),
        column("review", "待验证", OnEnterKind::HumanGate, None, Some("done"), Some("implementing")),
        column("done", "完成", OnEnterKind::None, None, None, None),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_template_matches_design_flow() {
        let columns = default_template();
        validate_columns(&columns).unwrap();
        let ids: Vec<&str> = columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["requirements", "implementing", "testing", "review", "done"]);
        assert_eq!(columns[0].on_enter.kind, OnEnterKind::HumanGate);
        assert_eq!(columns[1].on_enter.kind, OnEnterKind::AgentRun);
        assert_eq!(columns[1].on_enter.agent.as_deref(), Some("execution"));
        assert_eq!(columns[2].on_enter.agent.as_deref(), Some("qa"));
        assert_eq!(columns[3].on_enter.kind, OnEnterKind::HumanGate);
        // 流转表：测试失败回流实现中，待验证拒绝回流实现中，完成列终态无出边
        let table: Vec<(Option<&str>, Option<&str>)> =
            columns.iter().map(|c| (c.transitions.on_success.as_deref(), c.transitions.on_failure.as_deref())).collect();
        assert_eq!(
            table,
            [
                (Some("implementing"), None),
                (Some("testing"), Some("requirements")),
                (Some("review"), Some("implementing")),
                (Some("done"), Some("implementing")),
                (None, None),
            ]
        );
    }

    #[test]
    fn column_validation_rejects_misconfiguration() {
        let mut column = ColumnDef {
            id: "bad".into(),
            title: "坏列".into(),
            on_enter: OnEnter { kind: OnEnterKind::AgentRun, agent: None },
            transitions: Transitions::default(),
            wip_limit: None,
            timeout_ms: None,
        };
        assert!(matches!(column.validate(), Err(KanbanError::InvalidColumn(_))), "agent_run 缺 agent 引用必须拒绝");
        column.on_enter = OnEnter { kind: OnEnterKind::HumanGate, agent: Some("x".into()) };
        assert!(matches!(column.validate(), Err(KanbanError::InvalidColumn(_))));
        column.on_enter = OnEnter::default();
        column.wip_limit = Some(0);
        assert!(matches!(column.validate(), Err(KanbanError::InvalidColumn(_))));
        column.wip_limit = Some(1);
        column.id = "../escape".into();
        assert!(matches!(column.validate(), Err(KanbanError::InvalidId(_))));
    }

    #[test]
    fn validate_columns_rejects_dangling_transition_target() {
        let mut columns = default_template();
        columns[1].transitions.on_failure = Some("nowhere".into());
        assert!(matches!(validate_columns(&columns), Err(KanbanError::ColumnNotFound(_))));
    }
}
