//! kanban RPC 方法（kanban.{boards,snapshot,board_create,card_create,card_move,card_comment,run_start,policy_set}）。
//! 变更方法先落事件流再 publish KanbanUpdate（commit_and_publish，同 goal_rpc 口径）。
//! workspace 校验 fail-closed：路径必须逐字命中注册列表，RPC 不能读写任意目录的 .agents/kxen。

use std::path::{Path, PathBuf};

use kxen_core::core::event::{Event, EventBus};
use kxen_core::core::ids;
use kxen_core::kanban::{Board, EventKind, KanbanCommand, KanbanError, KanbanEvent, Outcome, PolicySpec};
use serde_json::{Value, json};

pub async fn call(method: &str, params: Value, state: &std::sync::Arc<crate::AppState>) -> Result<Value, String> {
    match method {
        "kanban.boards" => Ok(json!(boards(&checked_workspace(&params)?))),
        "kanban.snapshot" => {
            let workspace = checked_workspace(&params)?;
            snapshot_body(&workspace, &board_id(&params)?)
        }
        "kanban.board_create" => {
            let workspace = checked_workspace(&params)?;
            let title = params.get("title").and_then(Value::as_str).ok_or("missing title")?;
            let columns = params.get("columns").cloned().map(serde_json::from_value).transpose().map_err(|e| e.to_string())?;
            let board_id = ids::new_id("board");
            let event = apply_and_publish(&state.bus, &workspace, &board_id, KanbanCommand::BoardCreate { title: title.into(), columns })?;
            Ok(json!({ "event_id": event.id, "seq": event.seq, "board_id": board_id }))
        }
        "kanban.card_create" => with_board(&params, |workspace, board_id| {
            let title = params.get("title").and_then(Value::as_str).ok_or("missing title")?;
            let body = params.get("body").and_then(Value::as_str).unwrap_or_default().to_string();
            let column_id = params.get("column_id").and_then(Value::as_str).map(String::from);
            let event =
                apply_and_publish(&state.bus, workspace, board_id, KanbanCommand::CardCreate { column_id, title: title.into(), body })?;
            let EventKind::CardCreate(payload) = event.kind else { return Err("card_create returned unexpected event".into()) };
            Ok(json!({ "event_id": event.id, "seq": event.seq, "card_id": payload.card_id }))
        }),
        "kanban.card_move" => with_board(&params, |workspace, board_id| {
            // 人工 approve/reject 的入口；timeout 没有人工语义（只能由 run_timeout 事件产生），解析期即拒
            let outcome = parse_outcome(&params)?;
            let command = KanbanCommand::CardMove { card_id: card_id(&params)?, outcome };
            apply_and_publish(&state.bus, workspace, board_id, command).map(|event| landed(&event))
        }),
        "kanban.card_comment" => with_board(&params, |workspace, board_id| {
            let body = params.get("body").and_then(Value::as_str).ok_or("missing body")?;
            // 默认 human：与工具面的 "agent" 区分来源，评论列表据此标色
            let author = params.get("author").and_then(Value::as_str).unwrap_or("human").to_string();
            let command = KanbanCommand::CardComment { card_id: card_id(&params)?, author, body: body.into() };
            apply_and_publish(&state.bus, workspace, board_id, command).map(|event| landed(&event))
        }),
        "kanban.run_start" => with_board(&params, |workspace, board_id| {
            // blocked/超时卡的显式重试入口：落 run_started 事件，runner 周期扫描收养执行
            let command = KanbanCommand::RunStarted { card_id: card_id(&params)? };
            apply_and_publish(&state.bus, workspace, board_id, command).map(|event| landed(&event))
        }),
        "kanban.policy_set" => with_board(&params, |workspace, board_id| {
            let policy: PolicySpec =
                serde_json::from_value(params.get("policy").cloned().ok_or("missing policy")?).map_err(|e| e.to_string())?;
            apply_and_publish(&state.bus, workspace, board_id, KanbanCommand::PolicySet { policy }).map(|event| landed(&event))
        }),
        other => Err(format!("unknown kanban method: {other}")),
    }
}

/// boards 列表 = digest 计数（与 overview 单一口径）+ policy 摘要。
/// policy 只在 RPC 读第二遍（UI 授权编辑需要当前状态）；overview 卡片只要计数徽标，不多读。
fn boards(workspace: &Path) -> Vec<Value> {
    kxen_core::kanban::collect(workspace)
        .iter()
        .map(|digest| {
            let policy = Board::open(workspace, &digest.board_id).ok().and_then(|board| board.state().policy.clone()).map(|policy| {
                json!({
                    "allowlist": policy.spec.allowlist.len(),
                    "used": policy.used,
                    "max_uses": policy.spec.max_uses,
                    "expires_at_ms": policy.spec.expires_at_ms,
                })
            });
            json!({
                "board_id": digest.board_id,
                "title": digest.title,
                "total_cards": digest.total_cards,
                "waiting_human": digest.waiting_human,
                "running": digest.running,
                "blocked": digest.blocked,
                "policy": policy,
            })
        })
        .collect()
}

/// 完整 BoardState JSON（重连恢复用，对标 approval.pending）：fail-closed，读失败即报错。
fn snapshot_body(workspace: &Path, board_id: &str) -> Result<Value, String> {
    let board = Board::open(workspace, board_id).map_err(|e| e.to_string())?;
    if !board.state().created() {
        return Err(KanbanError::BoardNotCreated(board_id.into()).to_string());
    }
    serde_json::to_value(board.state()).map_err(|e| e.to_string())
}

/// 变更收口：事件先落盘再广播（无订阅者静默丢弃，不算错误）。
fn apply_and_publish(bus: &EventBus, workspace: &Path, board_id: &str, command: KanbanCommand) -> Result<KanbanEvent, String> {
    let mut board = Board::open(workspace, board_id).map_err(|e| e.to_string())?;
    let event = board.apply(command).map_err(|e| e.to_string())?;
    bus.publish(Event::KanbanUpdate { board_id: board_id.into(), workspace: workspace.to_string_lossy().into_owned() });
    Ok(event)
}

fn landed(event: &KanbanEvent) -> Value {
    json!({ "event_id": event.id, "seq": event.seq })
}

fn with_board<T>(params: &Value, f: impl FnOnce(&Path, &str) -> Result<T, String>) -> Result<T, String> {
    let workspace = checked_workspace(params)?;
    let board_id = board_id(params)?;
    f(&workspace, &board_id)
}

fn checked_workspace(params: &Value) -> Result<PathBuf, String> {
    let workspace = params.get("workspace").and_then(Value::as_str).ok_or("missing workspace")?;
    checked_workspace_in(&kxen_core::core::paths::KxenPaths::user().root(), workspace)
}

/// 逐字命中才算注册：尾随斜杠、相对写法、前缀相似路径一律拒绝（不 canonicalize 猜测）。
fn checked_workspace_in(data_dir: &Path, workspace: &str) -> Result<PathBuf, String> {
    let registered = kxen_core::core::workspace::list(data_dir).map_err(|error| error.to_string())?;
    if !registered.iter().any(|w| w.path == workspace) {
        return Err(format!("workspace not registered: {workspace}"));
    }
    Ok(PathBuf::from(workspace))
}

fn board_id(params: &Value) -> Result<String, String> {
    let id = params.get("board").and_then(Value::as_str).ok_or("missing board")?;
    ids::validate_id(id)?;
    Ok(id.to_string())
}

fn card_id(params: &Value) -> Result<String, String> {
    let id = params.get("card_id").and_then(Value::as_str).ok_or("missing card_id")?;
    ids::validate_id(id)?;
    Ok(id.to_string())
}

fn parse_outcome(params: &Value) -> Result<Outcome, String> {
    let outcome = params.get("outcome").and_then(Value::as_str).ok_or("missing outcome")?;
    // 不用 match 字面量臂：rpc_contract 门禁把「"..." =>」形式当方法名对账
    if outcome == "success" {
        return Ok(Outcome::Success);
    }
    if outcome == "failure" {
        return Ok(Outcome::Failure);
    }
    Err(format!("invalid outcome: {outcome}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("kxen-kanban-rpc-{tag}-{}-{nanos}", std::process::id()))
    }

    fn create_board(workspace: &Path, board: &str) -> Board {
        let mut board_handle = Board::open(workspace, board).unwrap();
        board_handle.apply(KanbanCommand::BoardCreate { title: "交付板".into(), columns: None }).unwrap();
        board_handle
    }

    #[test]
    fn unregistered_workspace_is_rejected_fail_closed() {
        let data = temp("data");
        let workspace = temp("ws");
        let path = workspace.to_string_lossy().into_owned();
        assert!(checked_workspace_in(&data, &path).is_err(), "未注册路径必须拒绝");
        kxen_core::core::workspace::touch(&data, &path).unwrap();
        assert_eq!(checked_workspace_in(&data, &path).unwrap(), workspace);
        assert!(checked_workspace_in(&data, &format!("{path}/")).is_err(), "逐字命中：尾随斜杠算未注册");
        assert!(checked_workspace_in(&data, &format!("{path}x")).is_err(), "前缀相似路径算未注册");
        std::fs::remove_dir_all(&data).ok();
        std::fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn boards_is_best_effort_and_carries_policy_summary() {
        let workspace = temp("boards");
        let mut board = create_board(&workspace, "board_a");
        board
            .apply(KanbanCommand::PolicySet {
                policy: PolicySpec { allowlist: vec!["cargo".into()], max_uses: Some(3), expires_at_ms: None },
            })
            .unwrap();
        // 坏板（events.jsonl 无法解析）不拖累整表
        let bad_dir = kxen_core::kanban::board_dir(&workspace, "board_bad").unwrap();
        std::fs::create_dir_all(&bad_dir).unwrap();
        std::fs::write(kxen_core::kanban::events_path(&bad_dir), "{broken\n").unwrap();

        let list = boards(&workspace);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["board_id"], json!("board_a"));
        assert_eq!(list[0]["title"], json!("交付板"));
        assert_eq!(list[0]["policy"], json!({ "allowlist": 1, "used": 0, "max_uses": 3, "expires_at_ms": null }));
        std::fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn snapshot_returns_full_board_state() {
        let workspace = temp("snapshot");
        let mut board = create_board(&workspace, "board_s");
        board.apply(KanbanCommand::CardCreate { column_id: None, title: "加登录".into(), body: "详情".into() }).unwrap();

        let snapshot = snapshot_body(&workspace, "board_s").unwrap();
        assert_eq!(snapshot["board_id"], json!("board_s"));
        assert_eq!(snapshot["title"], json!("交付板"));
        assert_eq!(snapshot["columns"].as_array().unwrap().len(), 5, "默认模板五列");
        let cards = snapshot["cards"].as_object().unwrap();
        assert_eq!(cards.len(), 1);
        let card = cards.values().next().unwrap();
        assert_eq!(card["title"], json!("加登录"));
        assert_eq!(card["status"], json!("waiting_human"));
        assert_eq!(card["comments"], json!([]));
        assert_eq!(snapshot["seq"], json!(2));
        assert!(snapshot.get("policy").is_none(), "未授权时 policy 字段缺省（skip_serializing_if）");

        assert!(snapshot_body(&workspace, "board_missing").is_err(), "未创建的板 fail-closed");
        std::fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn card_id_rejects_invalid_id_format() {
        assert_eq!(card_id(&json!({ "card_id": "card_1" })).unwrap(), "card_1");
        assert!(card_id(&json!({ "card_id": "bad id" })).is_err(), "非法字符必须拒绝（同 board_id 口径）");
        assert!(card_id(&json!({ "card_id": "../escape" })).is_err(), "路径穿越写法必须拒绝");
        assert!(card_id(&json!({})).is_err(), "缺 card_id 拒绝");
    }

    #[test]
    fn card_move_rejects_timeout_outcome() {
        assert!(matches!(parse_outcome(&json!({ "outcome": "success" })), Ok(Outcome::Success)));
        assert!(matches!(parse_outcome(&json!({ "outcome": "failure" })), Ok(Outcome::Failure)));
        assert!(parse_outcome(&json!({ "outcome": "timeout" })).is_err(), "timeout 只能由 run_timeout 事件产生");
        assert!(parse_outcome(&json!({})).is_err(), "缺 outcome 拒绝");
    }

    #[test]
    fn mutation_publishes_kanban_update_after_commit() {
        let bus = EventBus::new(4);
        let mut receiver = bus.subscribe();
        let workspace = temp("publish");

        let event =
            apply_and_publish(&bus, &workspace, "board_p", KanbanCommand::BoardCreate { title: "板".into(), columns: None }).unwrap();
        assert_eq!(event.seq, 1);

        match receiver.try_recv().expect("变更成功后必须广播") {
            Event::KanbanUpdate { board_id, workspace: ws } => {
                assert_eq!(board_id, "board_p");
                assert_eq!(ws, workspace.to_string_lossy());
            }
            _ => panic!("unexpected event kind"),
        }
        std::fs::remove_dir_all(&workspace).ok();
    }
}
