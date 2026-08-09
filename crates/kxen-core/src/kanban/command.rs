//! 命令守卫：模型/外部只交意图（KanbanCommand），校验通过才提交事件，模型不能直写状态。
//! 提交顺序对齐 goal_tool 的 load -> 校验 -> save -> publish 串行化（事件化后 save 替换为 append）：
//! 校验（纯读内存投影）-> append 事件（先落盘）-> 更新内存投影 -> 刷新快照缓存。
//! 非法命令 fail-closed：返回结构化错误，事件流与投影零副作用。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use crate::core::ids;
use crate::core::shared::now_ms;

use super::error::KanbanError;
use super::events::*;
use super::model::{CardStatus, OnEnterKind, default_template, validate_columns};
use super::projection::{self, BoardState};
use super::store;

/// per-board 进程内锁（同 goal::write_lock 模式）：open -> 校验 -> append 串行化，并发调用不互相覆盖。
pub fn board_lock(board_id: &str) -> Arc<Mutex<()>> {
    static LOCKS: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
    crate::core::shared::lock(&LOCKS).entry(board_id.to_string()).or_default().clone()
}

pub struct Board {
    dir: PathBuf,
    state: BoardState,
}

impl Board {
    /// 打开（或定位）一个 board：从事件流重建状态，board 未创建时返回空投影，由 BoardCreate 命令建立。
    pub fn open(workspace: &Path, board_id: &str) -> Result<Self, KanbanError> {
        let dir = store::board_dir(workspace, board_id)?;
        let state = store::load_state(workspace, board_id)?;
        Ok(Self { dir, state })
    }

    pub fn state(&self) -> &BoardState {
        &self.state
    }

    pub fn apply(&mut self, command: KanbanCommand) -> Result<KanbanEvent, KanbanError> {
        let lock = board_lock(&self.state.board_id);
        let _guard = crate::core::shared::lock(&lock);
        // 锁顺序固定：进程内 board_lock -> 文件锁（全代码库只此一处同时持两把，单向顺序无死锁面）。
        // 文件锁持到函数结束，覆盖预检/校验/append/reduce/快照写全程，挡住另一进程的同时 apply
        let _file_guard = store::lock_events(&self.dir)?;
        // 锁内漂移预检：锁外写入者（另一进程/绕开共享锁的实例）推进或等长重写事件流时先补折再校验，
        // 否则 validate 用过期投影放行，非法事件已 durable 才报 divergence。比对 (seq, 尾事件 id)
        // 内容锚而非纯 seq：等长重写 seq 不变但 id 不同，纯 seq 漏检会把错状态洗白进快照
        let on_disk = store::last_event_anchor(&store::events_path(&self.dir))?;
        let projected = if self.state.seq == 0 { None } else { self.state.anchor_event_id.clone().map(|anchor| (self.state.seq, anchor)) };
        if on_disk != projected {
            self.state = store::load_state_from_dir(&self.dir, &self.state.board_id)?;
        }
        let kind = self.validate(&command)?;
        let mut event = KanbanEvent { id: ids::new_id("kev"), board_id: self.state.board_id.clone(), seq: 0, created_at: now_ms(), kind };
        let expected_anchor = if self.state.seq == 0 {
            None
        } else {
            Some((self.state.seq, self.state.anchor_event_id.as_deref().expect("non-empty projection has an anchor")))
        };
        store::append_event_at(&store::events_path(&self.dir), &mut event, expected_anchor)?;
        // append 指派的 seq 必须紧跟内存投影；否则存在绕过锁的写入者，fail-closed 要求重开而非猜测
        if event.seq != self.state.seq + 1 {
            return Err(KanbanError::Log(format!("event log diverged from projection at seq {}", event.seq)));
        }
        projection::reduce(&mut self.state, &event)?;
        drop(_file_guard);
        drop(_guard);
        // 快照是纯缓存：写失败不影响已提交事件，下次启动从事件流重建
        if let Err(error) = store::save_snapshot(&self.dir, &self.state) {
            tracing::warn!(%error, "kanban snapshot cache write failed");
        }
        Ok(event)
    }

    fn validate(&self, command: &KanbanCommand) -> Result<EventKind, KanbanError> {
        match command {
            KanbanCommand::BoardCreate { title, columns } => {
                if self.state.created() {
                    return Err(KanbanError::BoardExists(self.state.board_id.clone()));
                }
                let title = title.trim();
                if title.is_empty() {
                    return Err(KanbanError::InvalidCommand("board title is required".into()));
                }
                let columns = match columns {
                    Some(columns) => columns.clone(),
                    None => default_template(),
                };
                validate_columns(&columns)?;
                Ok(EventKind::BoardCreate(BoardCreatePayload { title: title.into(), columns }))
            }
            KanbanCommand::ColumnAdd { column } => {
                self.require_created()?;
                column.validate()?;
                if self.state.column(&column.id).is_some() {
                    return Err(KanbanError::ColumnExists(column.id.clone()));
                }
                for target in [&column.transitions.on_success, &column.transitions.on_failure].into_iter().flatten() {
                    if self.state.column(target).is_none() {
                        return Err(KanbanError::ColumnNotFound(target.clone()));
                    }
                }
                Ok(EventKind::ColumnAdd(ColumnAddPayload { column: column.clone() }))
            }
            KanbanCommand::CardCreate { column_id, title, body } => {
                self.require_created()?;
                let title = title.trim();
                if title.is_empty() {
                    return Err(KanbanError::InvalidCommand("card title is required".into()));
                }
                let column = match column_id {
                    Some(id) => self.state.column(id).ok_or_else(|| KanbanError::ColumnNotFound(id.clone()))?,
                    None => self.state.columns.first().ok_or_else(|| KanbanError::InvalidCommand("board has no columns".into()))?,
                };
                self.check_wip(&column.id, None)?;
                Ok(EventKind::CardCreate(CardCreatePayload {
                    card_id: ids::new_id("card"),
                    column_id: column.id.clone(),
                    title: title.into(),
                    body: body.clone(),
                }))
            }
            KanbanCommand::CardMove { card_id, outcome } => {
                self.require_created()?;
                if *outcome == Outcome::Timeout {
                    return Err(KanbanError::InvalidCommand("card_move outcome must be success or failure".into()));
                }
                let card = self.state.cards.get(card_id).ok_or_else(|| KanbanError::CardNotFound(card_id.clone()))?;
                if card.status == CardStatus::Running {
                    return Err(KanbanError::RunInProgress(card_id.clone()));
                }
                let from = card.column_id.clone();
                let column = self.state.column(&from).ok_or_else(|| KanbanError::ColumnNotFound(from.clone()))?;
                // 目标列只能由流转表推导，调用方自报目标会被拒绝在类型层之外
                let to = column
                    .transitions
                    .target(*outcome)
                    .ok_or_else(|| KanbanError::NoTransition { card_id: card_id.clone(), from: from.clone(), outcome: *outcome })?
                    .to_string();
                if self.state.column(&to).is_none() {
                    return Err(KanbanError::ColumnNotFound(to));
                }
                self.check_wip(&to, Some(card_id))?;
                Ok(EventKind::CardMove(CardMovePayload { card_id: card_id.clone(), from, to, outcome: *outcome }))
            }
            KanbanCommand::CardComment { card_id, author, body } => {
                self.require_created()?;
                if !self.state.cards.contains_key(card_id) {
                    return Err(KanbanError::CardNotFound(card_id.clone()));
                }
                if author.trim().is_empty() || body.trim().is_empty() {
                    return Err(KanbanError::InvalidCommand("comment author and body are required".into()));
                }
                Ok(EventKind::CardComment(CardCommentPayload { card_id: card_id.clone(), author: author.clone(), body: body.clone() }))
            }
            KanbanCommand::RunStarted { card_id } => {
                self.require_created()?;
                let card = self.state.cards.get(card_id).ok_or_else(|| KanbanError::CardNotFound(card_id.clone()))?;
                if card.status == CardStatus::Running {
                    return Err(KanbanError::RunInProgress(card_id.clone()));
                }
                let column = self.state.column(&card.column_id).ok_or_else(|| KanbanError::ColumnNotFound(card.column_id.clone()))?;
                if !matches!(column.on_enter.kind, OnEnterKind::AgentRun | OnEnterKind::Workflow) {
                    return Err(KanbanError::InvalidCommand(format!("column {} has no agent_run/workflow on_enter", column.id)));
                }
                let attempt =
                    self.state.runs.values().filter(|run| run.card_id == *card_id && run.column_id == column.id).count() as u32 + 1;
                // run_id 按 design.md 派生：board_id:card_id:column_id:attempt（workflow journal 同形）
                let run_id = format!("{}:{}:{}:{}", self.state.board_id, card_id, column.id, attempt);
                Ok(EventKind::RunStarted(RunStartedPayload { run_id, card_id: card_id.clone(), column_id: column.id.clone(), attempt }))
            }
            KanbanCommand::RunFinished { run_id, outcome } => {
                self.require_created()?;
                if *outcome == Outcome::Timeout {
                    return Err(KanbanError::InvalidCommand("use run_timeout command for timeouts".into()));
                }
                let run = self.open_run(run_id)?;
                // 投影按 run 所在列 transitions 推导迁移目标，守卫对同一目标做 WIP 检查
                if let Some(to) = self.state.column(&run.column_id).and_then(|c| c.transitions.target(*outcome)).map(str::to_string) {
                    self.check_wip(&to, Some(&run.card_id))?;
                }
                Ok(EventKind::RunFinished(RunFinishedPayload { run_id: run_id.clone(), outcome: *outcome }))
            }
            KanbanCommand::RunTimeout { run_id } => {
                self.require_created()?;
                self.open_run(run_id)?;
                Ok(EventKind::RunTimeout(RunTimeoutPayload { run_id: run_id.clone() }))
            }
            KanbanCommand::AgentDefined { name, role, model, permission_profile, tools } => {
                self.require_created()?;
                // 同名重复定义是有意的 redefine 语义：AI 迭代修改定义依赖静默覆盖，
                // 此处不得加重复守卫（投影侧 BTreeMap insert 同为覆盖）
                ids::validate_id(name).map_err(KanbanError::InvalidId)?;
                if role.trim().is_empty() || model.trim().is_empty() || permission_profile.trim().is_empty() {
                    return Err(KanbanError::InvalidCommand("agent role, model and permission_profile are required".into()));
                }
                // tools 与 profile 单一口径（与 agents.rs parse 同守卫，事件层独立把守）：
                // custom 必须显式工具集且过闭集；其余 profile 自带 tools = 两种权限口径并存，拒绝
                match (permission_profile.as_str(), tools) {
                    ("custom", Some(tools)) => super::agents::validate_custom_tools(tools)?,
                    ("custom", None) => return Err(KanbanError::InvalidAgentDef("custom permission_profile requires tools".into())),
                    (_, Some(_)) => return Err(KanbanError::InvalidAgentDef("tools is only valid with permission_profile custom".into())),
                    (_, None) => {}
                }
                Ok(EventKind::AgentDefined(AgentDefinedPayload {
                    name: name.clone(),
                    role: role.clone(),
                    model: model.clone(),
                    permission_profile: permission_profile.clone(),
                    tools: tools.clone(),
                }))
            }
            KanbanCommand::PolicySet { policy } => {
                self.require_created()?;
                // 匹配语义以 trim 后的前缀为准，存储的即 trim 后的：配置里的装饰空白不该改变授权面
                // （尾空格前缀会因词边界判定永不命中，方向安全但用户困惑）
                let allowlist: Vec<String> = policy.allowlist.iter().map(|prefix| prefix.trim().to_string()).collect();
                if allowlist.is_empty() || allowlist.iter().any(String::is_empty) {
                    return Err(KanbanError::InvalidCommand("policy allowlist must be non-empty command prefixes".into()));
                }
                // max_uses = 0 等于设了立即失效，只能是误配
                if policy.max_uses == Some(0) {
                    return Err(KanbanError::InvalidCommand("policy max_uses must be >= 1".into()));
                }
                // 守卫可读墙钟（投影保持纯函数）：设了就已过期只能是误配
                if policy.expires_at_ms.is_some_and(|expires| expires <= now_ms()) {
                    return Err(KanbanError::InvalidCommand("policy expires_at_ms must be in the future".into()));
                }
                let mut policy = policy.clone();
                policy.allowlist = allowlist;
                Ok(EventKind::PolicySet(PolicySetPayload { policy }))
            }
            KanbanCommand::AutoApproved { run_id, command } => {
                self.require_created()?;
                // 计数与放行的原子点（board_lock 已串行化）：全部条件同一次守卫内判定，通过才转事件
                let policy = self.state.policy.as_ref().ok_or_else(|| KanbanError::PolicyDenied("no active policy".into()))?;
                if policy.spec.expires_at_ms.is_some_and(|expires| now_ms() > expires) {
                    return Err(KanbanError::PolicyDenied("policy expired".into()));
                }
                if let Some(max) = policy.spec.max_uses
                    && policy.used >= max
                {
                    return Err(KanbanError::PolicyDenied(format!("policy exhausted ({}/{max})", policy.used)));
                }
                let command_head = command.trim_start();
                // 词边界：prefix 之后必须是串结束或 ASCII 空白，否则 "git" 会放行 "gitx upload"
                let prefix_hit = policy.spec.allowlist.iter().any(|prefix| {
                    command_head
                        .strip_prefix(prefix.as_str())
                        .is_some_and(|rest| rest.is_empty() || rest.as_bytes()[0].is_ascii_whitespace())
                });
                if !prefix_hit {
                    return Err(KanbanError::PolicyDenied("command matches no allowlist prefix".into()));
                }
                // 复合命令不可自动放行：元字符意味着前缀命中之外还藏着第二段动作
                // （; & | 换行 反引号 $ 括号 重定向 反斜杠）。拒绝 = 回落逐次审批，不是硬拒执行
                if command_head
                    .bytes()
                    .any(|b| matches!(b, b';' | b'&' | b'|' | b'\n' | b'\r' | b'`' | b'$' | b'(' | b')' | b'<' | b'>' | b'\\'))
                {
                    return Err(KanbanError::PolicyDenied("command contains shell metacharacters".into()));
                }
                self.open_run(run_id)?;
                Ok(EventKind::AutoApproved(AutoApprovedPayload { run_id: run_id.clone(), command: command.clone() }))
            }
        }
    }

    fn require_created(&self) -> Result<(), KanbanError> {
        if self.state.created() { Ok(()) } else { Err(KanbanError::BoardNotCreated(self.state.board_id.clone())) }
    }

    fn open_run(&self, run_id: &str) -> Result<&super::model::RunState, KanbanError> {
        match self.state.runs.get(run_id) {
            Some(run) if run.outcome.is_none() => Ok(run),
            _ => Err(KanbanError::RunNotOpen(run_id.to_string())),
        }
    }

    /// WIP 口径：列内全部卡片计数（含 blocked/waiting，对齐看板惯例）；exclude 用于迁出卡自身的同列自环。
    fn check_wip(&self, column_id: &str, exclude: Option<&str>) -> Result<(), KanbanError> {
        let Some(limit) = self.state.column(column_id).and_then(|c| c.wip_limit) else { return Ok(()) };
        let count = self.state.cards.values().filter(|card| card.column_id == column_id && Some(card.id.as_str()) != exclude).count();
        if count >= limit as usize {
            return Err(KanbanError::WipLimit { column: column_id.to_string(), limit });
        }
        Ok(())
    }
}

#[cfg(test)]
mod agent_tools_tests;
#[cfg(test)]
mod drift_tests;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod tests;
