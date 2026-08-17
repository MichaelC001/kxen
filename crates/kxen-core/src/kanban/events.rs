//! Kanban 命令与事件类型。看板状态的唯一真源是 append-only 事件流，
//! 解析 fail-closed：未知 type、缺字段、未知字段一律拒绝，半截事件不进投影。
//!
//! payload 用强类型而非 serde_json::Value：投影是纯函数，形状错误必须在解析期暴露；
//! Value 会把校验推给投影（fail-open），还允许语义空事件落盘，破坏可回放性。

use serde::{Deserialize, Serialize};

use super::model::{ColumnDef, PolicySpec};

/// 迁移/执行结果收口：human approve = Success、reject = Failure；Timeout 只能由 run_timeout 事件产生。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failure,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardCreatePayload {
    pub title: String,
    pub columns: Vec<ColumnDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnAddPayload {
    pub column: ColumnDef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardCreatePayload {
    pub card_id: String,
    pub column_id: String,
    pub title: String,
    pub body: String,
}

/// from/to 落进事件：投影重放时校验 from 与卡片当前列一致，事件流自相矛盾即 fail-closed。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardMovePayload {
    pub card_id: String,
    pub from: String,
    pub to: String,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardCommentPayload {
    pub card_id: String,
    pub author: String,
    pub body: String,
}

/// run_id 按 design.md 派生：`board_id:card_id:column_id:attempt`（workflow 衔接时同形，命中 journal 缓存）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunStartedPayload {
    pub run_id: String,
    pub card_id: String,
    pub column_id: String,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunFinishedPayload {
    pub run_id: String,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunTimeoutPayload {
    pub run_id: String,
}

/// Agent 定义文件本体存 `.agents/kxen/kanban/agents/*.md`（P2），事件只登记元数据，重放不依赖文件内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDefinedPayload {
    pub name: String,
    pub role: String,
    pub model: String,
    pub permission_profile: String,
    /// custom profile 的显式工具白名单（固定三档恒 None）；旧事件流无此字段，serde default 兼容重放。
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

/// 自主授权配置本体进事件：投影重建不依赖任何外部状态，重放即恢复授权。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicySetPayload {
    pub policy: PolicySpec,
}

/// 自动放行审计：命令本体进事件（放行决策的可回放证据），计数由投影推导。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoApprovedPayload {
    pub run_id: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventKind {
    BoardCreate(BoardCreatePayload),
    ColumnAdd(ColumnAddPayload),
    CardCreate(CardCreatePayload),
    CardMove(CardMovePayload),
    CardComment(CardCommentPayload),
    RunStarted(RunStartedPayload),
    RunFinished(RunFinishedPayload),
    RunTimeout(RunTimeoutPayload),
    AgentDefined(AgentDefinedPayload),
    PolicySet(PolicySetPayload),
    AutoApproved(AutoApprovedPayload),
}

/// seq 由 store 在 append 时指派（从 1 连续递增），调用方不得预设。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KanbanEvent {
    pub id: String,
    pub board_id: String,
    pub seq: u64,
    pub created_at: u64,
    pub kind: EventKind,
}

/// 意图层：模型/外部只能提交 Command，由 core 校验后才转成 Event；不存在直写状态的路径。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum KanbanCommand {
    BoardCreate {
        title: String,
        columns: Option<Vec<ColumnDef>>,
    },
    ColumnAdd {
        column: ColumnDef,
    },
    CardCreate {
        column_id: Option<String>,
        title: String,
        body: String,
    },
    /// 迁移目标由当前列 transitions[outcome] 推导，调用方不能自报目标列（防止绕过流转表）。
    CardMove {
        card_id: String,
        outcome: Outcome,
    },
    CardComment {
        card_id: String,
        author: String,
        body: String,
    },
    RunStarted {
        card_id: String,
    },
    RunFinished {
        run_id: String,
        outcome: Outcome,
    },
    RunTimeout {
        run_id: String,
    },
    AgentDefined {
        name: String,
        role: String,
        model: String,
        permission_profile: String,
        /// custom 必填（闭集校验过）；固定三档必须为 None（权限语义单一来源）。
        tools: Option<Vec<String>>,
    },
    /// human-only：看板级自主授权（重设即重置计数，是显式续期语义）；不进模型工具目录。
    PolicySet {
        policy: PolicySpec,
    },
    /// 仅由 BoardAutoApprove 在放行时提交：守卫全过才转事件，这条命令就是计数与放行的原子点。
    AutoApproved {
        run_id: String,
        // serde 键避开 enum tag「command」：同键冲突在类型层即被拒绝
        #[serde(rename = "shell_command")]
        command: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event_json() -> String {
        let event = KanbanEvent {
            id: "kev_1".into(),
            board_id: "board_1".into(),
            seq: 1,
            created_at: 42,
            kind: EventKind::CardMove(CardMovePayload {
                card_id: "card_1".into(),
                from: "requirements".into(),
                to: "implementing".into(),
                outcome: Outcome::Success,
            }),
        };
        serde_json::to_string(&event).unwrap()
    }

    #[test]
    fn event_roundtrip_preserves_all_fields() {
        let json = sample_event_json();
        let parsed: KanbanEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
    }

    #[test]
    fn parse_rejects_unknown_event_type() {
        let bad = r#"{"id":"kev_1","board_id":"b","seq":1,"created_at":1,"kind":{"type":"card_teleport","payload":{}}}"#;
        assert!(serde_json::from_str::<KanbanEvent>(bad).is_err());
    }

    #[test]
    fn parse_rejects_missing_payload_field() {
        let bad = r#"{"id":"kev_1","board_id":"b","seq":1,"created_at":1,"kind":{"type":"run_finished","payload":{"run_id":"r"}}}"#;
        assert!(serde_json::from_str::<KanbanEvent>(bad).is_err(), "缺 outcome 必须拒绝");
    }

    #[test]
    fn parse_rejects_unknown_fields() {
        let bad_payload =
            r#"{"id":"kev_1","board_id":"b","seq":1,"created_at":1,"kind":{"type":"run_timeout","payload":{"run_id":"r","extra":1}}}"#;
        assert!(serde_json::from_str::<KanbanEvent>(bad_payload).is_err());
        let bad_envelope =
            r#"{"id":"kev_1","board_id":"b","seq":1,"created_at":1,"kind":{"type":"run_timeout","payload":{"run_id":"r"}},"extra":1}"#;
        assert!(serde_json::from_str::<KanbanEvent>(bad_envelope).is_err());
    }

    #[test]
    fn command_roundtrip_and_rejects_unknown_command() {
        let command = KanbanCommand::CardMove { card_id: "card_1".into(), outcome: Outcome::Failure };
        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(serde_json::from_str::<KanbanCommand>(&json).unwrap(), command);
        assert!(serde_json::from_str::<KanbanCommand>(r#"{"command":"card_delete","card_id":"c"}"#).is_err());
    }
}
