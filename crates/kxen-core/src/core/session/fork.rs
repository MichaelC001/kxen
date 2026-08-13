use super::*;

/// 兼容调用：从指定消息之后手动分叉。
pub fn fork(dir: &Path, id: &str, message_id: &str) -> std::io::Result<Session> {
    fork_with_options(dir, id, message_id, ForkPosition::After, ForkKind::Manual)
}

fn branch_root_id(dir: &Path, parent: &Session) -> String {
    if let Some(root) = &parent.branch_root_id {
        return root.clone();
    }
    let mut current = parent.clone();
    let mut seen = std::collections::HashSet::from([current.id.clone()]);
    while let Some(parent_id) = current.parent_id.as_deref() {
        if !seen.insert(parent_id.to_string()) {
            break;
        }
        let Ok(next) = load_meta(dir, parent_id) else {
            return parent_id.to_string();
        };
        current = next;
    }
    current.id
}

/// 从指定消息前或后创建独立 Session。完整 meta 与历史先落 staging，meta 最后发布作为 admission marker。
pub fn fork_with_options(dir: &Path, id: &str, message_id: &str, position: ForkPosition, kind: ForkKind) -> std::io::Result<Session> {
    let _source_transaction = mutation_transaction(dir, id)?;
    let parent = load_meta(dir, id)?;
    let messages = messages::load_messages_checked_unlocked(dir, id)?;
    let Some(index) = messages.iter().position(|message| message.id == message_id) else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("message not found: {message_id}")));
    };
    let source = &messages[index];
    let now = now_ms();
    let prefix_end = match position {
        ForkPosition::Before => index,
        ForkPosition::After => index + 1,
    };
    let title_prefix = match kind {
        ForkKind::Manual => "分支",
        ForkKind::Edit => "编辑分支",
        ForkKind::Rerun => "重新生成",
    };
    let root_id = branch_root_id(dir, &parent);
    let session = Session {
        id: crate::core::ids::new_id("ses"),
        title: format!("{title_prefix}: {}", parent.title.chars().take(24).collect::<String>()),
        directory: parent.directory,
        parent_id: Some(id.to_string()),
        branch_root_id: Some(root_id),
        fork_point: Some(ForkPoint {
            message_id: source.id.clone(),
            message_index: (index + 1) as u64,
            message_created_at: source.created_at,
            position,
        }),
        fork_kind: Some(kind),
        created_at: now,
        updated_at: now,
        message_revision: prefix_end as u64,
        pinned: false,
        sort_order: None,
        model: parent.model,
    };
    let _fork_transaction = mutation_transaction(dir, &session.id)?;
    let mut jsonl = Vec::new();
    for message in &messages[..prefix_end] {
        let mut cloned = message.clone();
        cloned.id = crate::core::ids::new_id("msg");
        cloned.session_id = session.id.clone();
        serde_json::to_writer(&mut jsonl, &cloned).map_err(std::io::Error::other)?;
        jsonl.push(b'\n');
    }
    let meta = serde_json::to_vec_pretty(&session)?;
    finish_commit(
        &session.id,
        storage::create_session_files(&meta_path(dir, &session.id), &meta, &messages_path(dir, &session.id), &jsonl),
    )?;
    Ok(session)
}
