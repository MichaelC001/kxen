//! kanban_* 工具执行（P2b 工具面）：参数 fail-closed 解析 -> 打开 workspace 的 Board ->
//! KanbanCommand -> Board::apply（守卫校验 + append 事件）-> 变更类工具 publish KanbanUpdate（P5）-> 结构化结果文本。
//! 模型只交意图不直写状态；守卫拒绝（流转表/WIP/重复/未建板）以 KanbanError 原文返回，
//! 原因可读，模型据此修正重试（与 goal_tool 的结构化错误形态一致）。

use serde::Deserialize;
use serde_json::Value;

use super::context::AgentContext;
use crate::core::ids;
use crate::kanban::{
    AgentDefinition, Board, BoardState, CardStatus, ColumnDef, EventKind, KanbanCommand, KanbanError, KanbanEvent, OnEnterKind, Outcome,
    agent_definition_to_markdown, parse_agent_definition, save_agent_definition,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoardCreateArgs {
    board: Option<String>,
    title: String,
    columns: Option<Vec<ColumnDef>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ColumnAddArgs {
    board: String,
    column: ColumnDef,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardCreateArgs {
    board: String,
    title: String,
    body: Option<String>,
    column_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardMoveArgs {
    board: String,
    card_id: String,
    outcome: Outcome,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardCommentArgs {
    board: String,
    card_id: String,
    body: String,
    author: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentCreateArgs {
    board: String,
    name: String,
    role: String,
    model: String,
    permission_profile: String,
    /// custom profile 的显式工具集（校验由 parse/save 与 command 双层把守）。
    tools: Option<Vec<String>>,
    prompt: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardRefArgs {
    board: String,
    card_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoardRefArgs {
    board: String,
}

/// 事件提交结果的统一锚点：事件 id/seq 供模型引用（回放与审计都按 seq 定位）。
fn landed(event: &KanbanEvent) -> String {
    format!("event {} seq {}", event.id, event.seq)
}

/// 同步执行：全部动作都是打开 Board + append 一条事件（fsync 级快速本地 IO），无 LLM/子进程，
/// 因此不在 needs_cancel_cleanup 名单——中断最坏情况是事件已落盘而结果未回传，
/// 事件幂等（append_event 同 id 去重），模型经 board_show 核实后重试安全。
pub fn execute_kanban_tool(name: &str, args: &Value, ctx: &AgentContext) -> Result<String, String> {
    let workspace = ctx.workdir.as_ref();
    match name {
        "kanban_board_create" => {
            let parsed: BoardCreateArgs = parse_args(name, args)?;
            let board_id = parsed.board.unwrap_or_else(|| ids::new_id("board"));
            let mut board = open(workspace, &board_id)?;
            let event = apply(&mut board, KanbanCommand::BoardCreate { title: parsed.title, columns: parsed.columns })?;
            publish(ctx, &board_id);
            let state = board.state();
            let columns =
                state.columns.iter().map(|column| format!("{} ({})", column.id, kind_name(column.on_enter.kind))).collect::<Vec<_>>();
            Ok(format!(
                "board created: {board_id} ({})\ntitle: {}\ncolumns: {}",
                landed(&event),
                state.title.as_deref().unwrap_or_default(),
                columns.join(", ")
            ))
        }
        "kanban_column_add" => {
            let parsed: ColumnAddArgs = parse_args(name, args)?;
            let column_id = parsed.column.id.clone();
            let mut board = open(workspace, &parsed.board)?;
            let event = apply(&mut board, KanbanCommand::ColumnAdd { column: parsed.column })?;
            publish(ctx, &parsed.board);
            Ok(format!("column added: {column_id} ({})", landed(&event)))
        }
        "kanban_card_create" => {
            let parsed: CardCreateArgs = parse_args(name, args)?;
            let mut board = open(workspace, &parsed.board)?;
            let event = apply(
                &mut board,
                KanbanCommand::CardCreate { column_id: parsed.column_id, title: parsed.title, body: parsed.body.unwrap_or_default() },
            )?;
            let EventKind::CardCreate(ref payload) = event.kind else { return Err("card_create returned unexpected event".into()) };
            publish(ctx, &parsed.board);
            Ok(format!("card created: {} in column {} ({})\ntitle: {}", payload.card_id, payload.column_id, landed(&event), payload.title))
        }
        "kanban_card_move" => {
            let parsed: CardMoveArgs = parse_args(name, args)?;
            let mut board = open(workspace, &parsed.board)?;
            let event = apply(&mut board, KanbanCommand::CardMove { card_id: parsed.card_id, outcome: parsed.outcome })?;
            let EventKind::CardMove(ref payload) = event.kind else { return Err("card_move returned unexpected event".into()) };
            publish(ctx, &parsed.board);
            Ok(format!(
                "card moved: {} {} -> {} (outcome {:?}, {})",
                payload.card_id,
                payload.from,
                payload.to,
                payload.outcome,
                landed(&event)
            ))
        }
        "kanban_card_comment" => {
            let parsed: CardCommentArgs = parse_args(name, args)?;
            let mut board = open(workspace, &parsed.board)?;
            let event = apply(
                &mut board,
                KanbanCommand::CardComment {
                    card_id: parsed.card_id.clone(),
                    author: parsed.author.unwrap_or_else(|| "agent".into()),
                    body: parsed.body,
                },
            )?;
            publish(ctx, &parsed.board);
            Ok(format!("comment added on {} ({})", parsed.card_id, landed(&event)))
        }
        "kanban_agent_create" => {
            let parsed: AgentCreateArgs = parse_args(name, args)?;
            let definition = AgentDefinition {
                name: parsed.name,
                role: parsed.role,
                model: parsed.model,
                permission_profile: parsed.permission_profile,
                tools: parsed.tools,
                prompt: parsed.prompt,
            };
            // 先按 save 的同一口径校验（四键/profile/tools/name id）：守卫失败零副作用（不落文件、不落事件）
            parse_agent_definition(&agent_definition_to_markdown(&definition)).map_err(|error| error.to_string())?;
            let mut board = open(workspace, &parsed.board)?;
            if !board.state().created() {
                return Err(KanbanError::BoardNotCreated(parsed.board).to_string());
            }
            // 定义文件是本体，先落盘（原子写）；随后 agent_defined 事件登记元数据（design.md 存储节）
            save_agent_definition(workspace, &definition).map_err(|error| error.to_string())?;
            let event = apply(
                &mut board,
                KanbanCommand::AgentDefined {
                    name: definition.name.clone(),
                    role: definition.role.clone(),
                    model: definition.model.clone(),
                    permission_profile: definition.permission_profile.clone(),
                    tools: definition.tools.clone(),
                },
            )?;
            publish(ctx, &parsed.board);
            Ok(format!(
                "agent defined: {} (role {}, model {}, profile {}, {})\ndefinition: .kxen/kanban/agents/{}.md",
                definition.name,
                definition.role,
                definition.model,
                definition.permission_profile,
                landed(&event),
                definition.name
            ))
        }
        "kanban_agent_run" => {
            let parsed: CardRefArgs = parse_args(name, args)?;
            let mut board = open(workspace, &parsed.board)?;
            // 显式 claim（run_started 先 durable）：runner 周期扫描收养 open claim 并经 driver 执行，
            // 工具本身不另起执行通道（runner.rs「显式重试」路径）
            let event = apply(&mut board, KanbanCommand::RunStarted { card_id: parsed.card_id })?;
            let EventKind::RunStarted(ref payload) = event.kind else { return Err("agent_run returned unexpected event".into()) };
            publish(ctx, &parsed.board);
            Ok(format!(
                "run claimed: {} ({})\ncolumn: {} attempt: {}\nthe kanban runner adopts claimed runs automatically; check kanban_board_show for the outcome",
                payload.run_id,
                landed(&event),
                payload.column_id,
                payload.attempt
            ))
        }
        "kanban_board_show" => {
            let parsed: BoardRefArgs = parse_args(name, args)?;
            let board = open(workspace, &parsed.board)?;
            if !board.state().created() {
                return Err(KanbanError::BoardNotCreated(parsed.board).to_string());
            }
            Ok(show_board(board.state()))
        }
        other => Err(format!("unknown kanban tool: {other}")),
    }
}

fn parse_args<T: serde::de::DeserializeOwned>(name: &str, args: &Value) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|error| format!("invalid arguments for tool {name}: {error}"))
}

fn open(workspace: &std::path::Path, board_id: &str) -> Result<Board, String> {
    Board::open(workspace, board_id).map_err(|error| error.to_string())
}

fn apply(board: &mut Board, command: KanbanCommand) -> Result<KanbanEvent, String> {
    board.apply(command).map_err(|error| error.to_string())
}

/// 变更成功后广播粗粒度信号：订阅了 kanban:<board_id> 的 UI 失效重拉 snapshot。
/// 只带 board_id/workspace 不带状态（snapshot 才是恢复口径）；无 bus 的子环境静默跳过。
fn publish(ctx: &AgentContext, board_id: &str) {
    if let Some(bus) = &ctx.bus {
        bus.publish(crate::core::event::Event::KanbanUpdate {
            board_id: board_id.into(),
            workspace: ctx.workdir.to_string_lossy().into_owned(),
        });
    }
}

fn kind_name(kind: OnEnterKind) -> &'static str {
    match kind {
        OnEnterKind::None => "none",
        OnEnterKind::AgentRun => "agent_run",
        OnEnterKind::Workflow => "workflow",
        OnEnterKind::HumanGate => "human_gate",
    }
}

fn status_name(status: CardStatus) -> &'static str {
    match status {
        CardStatus::Ready => "ready",
        CardStatus::WaitingHuman => "waiting_human",
        CardStatus::Running => "running",
        CardStatus::Blocked => "blocked",
    }
}

/// BoardState 的可读渲染（模型消费）：列/WIP/卡片状态/run 史/agent 注册表。
/// 遍历全部走 BTreeMap/列定义序，同一状态渲染确定。
fn show_board(state: &BoardState) -> String {
    let mut out = format!("board {} {:?} seq={}\n", state.board_id, state.title.as_deref().unwrap_or_default(), state.seq);
    out.push_str("columns:\n");
    for column in &state.columns {
        let cards: Vec<_> = state.cards.values().filter(|card| card.column_id == column.id).collect();
        let wip = match column.wip_limit {
            Some(limit) => format!("{}/{}", cards.len(), limit),
            None => cards.len().to_string(),
        };
        out.push_str(&format!("- {} {:?} on_enter={} wip={}\n", column.id, column.title, kind_name(column.on_enter.kind), wip));
        for card in cards {
            let run = card.current_run.as_deref().map(|run| format!(" run={run}")).unwrap_or_default();
            let blocked = card.block_reason.as_deref().map(|reason| format!(" blocked: {reason}")).unwrap_or_default();
            out.push_str(&format!("  * {} {:?} status={}{}{}\n", card.id, card.title, status_name(card.status), run, blocked));
        }
    }
    out.push_str("runs:\n");
    if state.runs.is_empty() {
        out.push_str("- none\n");
    }
    for run in state.runs.values() {
        let outcome = match run.outcome {
            Some(Outcome::Success) => "success",
            Some(Outcome::Failure) => "failure",
            Some(Outcome::Timeout) => "timeout",
            None => "open",
        };
        out.push_str(&format!("- {} card={} column={} attempt={} outcome={}\n", run.id, run.card_id, run.column_id, run.attempt, outcome));
    }
    out.push_str("agents:\n");
    if state.agents.is_empty() {
        out.push_str("- none\n");
    }
    for agent in state.agents.values() {
        out.push_str(&format!("- {} role={} model={} profile={}\n", agent.name, agent.role, agent.model, agent.permission_profile));
    }
    out.push_str("policy:\n");
    match &state.policy {
        None => out.push_str("- none\n"),
        Some(policy) => out.push_str(&format!(
            "- allowlist={} used={} max_uses={} expires_at_ms={}\n",
            policy.spec.allowlist.len(),
            policy.used,
            policy.spec.max_uses.map(|max| max.to_string()).unwrap_or_else(|| "none".into()),
            policy.spec.expires_at_ms.map(|expires| expires.to_string()).unwrap_or_else(|| "none".into()),
        )),
    }
    out.trim_end().to_string()
}

#[cfg(test)]
#[path = "kanban_tool/tests.rs"]
mod tests;
