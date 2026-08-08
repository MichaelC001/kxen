//! 看板摘要采集：Workspaces 卡片徽标（workspace overview）与 kanban.boards RPC 共用单一口径。

use std::path::Path;

use serde::Serialize;

use super::model::CardStatus;
use super::{Board, events_path};

/// 单块板的概览计数（卡片徽标/boards 列表共用形状）。
#[derive(Debug, Clone, Serialize)]
pub struct KanbanDigest {
    pub board_id: String,
    pub title: String,
    pub total_cards: usize,
    pub waiting_human: usize,
    pub running: usize,
    pub blocked: usize,
}

/// 尽力而为：digest 是列表入口，单块坏板/占位目录不能让整表空白；
/// fail-closed 属于快照 RPC 与命令路径（那里读失败必须报错）。
pub fn collect(workspace: &Path) -> Vec<KanbanDigest> {
    let mut digests = Vec::new();
    for board_id in list_boards(workspace).unwrap_or_default() {
        let Ok(board) = Board::open(workspace, &board_id) else {
            continue;
        };
        let state = board.state();
        // 未落 board_create 事件的目录只是占位（如 agents/ 附属目录），不算板
        if !state.created() {
            continue;
        }
        let count = |status: CardStatus| state.cards.values().filter(|card| card.status == status).count();
        digests.push(KanbanDigest {
            board_id,
            title: state.title.clone().unwrap_or_default(),
            total_cards: state.cards.len(),
            waiting_human: count(CardStatus::WaitingHuman),
            running: count(CardStatus::Running),
            blocked: count(CardStatus::Blocked),
        });
    }
    digests
}

/// 扫描 `<workspace>/.kxen/kanban/` 下的板：含 events.jsonl 的目录即板（agents/ 等附属目录自然排除）。
pub(crate) fn list_boards(workspace: &Path) -> Result<Vec<String>, String> {
    let root = workspace.join(".kxen").join("kanban");
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("read {}: {error}", root.display())),
    };
    let mut boards = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", root.display()))?;
        let path = entry.path();
        if path.is_dir()
            && events_path(&path).is_file()
            && let Some(name) = path.file_name().and_then(|name| name.to_str())
        {
            boards.push(name.to_string());
        }
    }
    boards.sort();
    Ok(boards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kanban::KanbanCommand;
    use crate::kanban::store;

    fn temp(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("kxen-kanban-digest-{tag}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn collect_counts_statuses_and_skips_bad_boards() {
        let workspace = temp("mixed");
        // 好板：一张 waiting_human 卡
        let mut board = Board::open(&workspace, "board_ok").unwrap();
        board.apply(KanbanCommand::BoardCreate { title: "好板".into(), columns: None }).unwrap();
        board.apply(KanbanCommand::CardCreate { column_id: None, title: "卡".into(), body: String::new() }).unwrap();
        // 坏板：events.jsonl 无法解析
        let bad_dir = store::board_dir(&workspace, "board_bad").unwrap();
        std::fs::create_dir_all(&bad_dir).unwrap();
        std::fs::write(store::events_path(&bad_dir), "{broken\n").unwrap();
        // 占位目录：有 events.jsonl 但没有 board_create 事件
        let empty_dir = store::board_dir(&workspace, "board_empty").unwrap();
        std::fs::create_dir_all(&empty_dir).unwrap();
        std::fs::write(store::events_path(&empty_dir), "").unwrap();

        let digests = collect(&workspace);
        assert_eq!(digests.len(), 1, "坏板与占位目录必须跳过，不拖累整表");
        assert_eq!(digests[0].board_id, "board_ok");
        assert_eq!(digests[0].title, "好板");
        assert_eq!(digests[0].total_cards, 1);
        assert_eq!(digests[0].waiting_human, 1);
        assert_eq!(digests[0].running, 0);
        assert_eq!(digests[0].blocked, 0);
        std::fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn missing_kanban_dir_is_empty_not_error() {
        let workspace = temp("none");
        std::fs::create_dir_all(&workspace).unwrap();
        assert!(collect(&workspace).is_empty());
        std::fs::remove_dir_all(&workspace).ok();
    }
}
