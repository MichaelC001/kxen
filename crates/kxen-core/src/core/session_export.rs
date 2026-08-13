//! 会话导出：markdown 渲染与落盘。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::core::session::{ForkKind, ForkPosition, Part, Role, load_messages_checked, load_meta, now_ms};

/// 导出 markdown：user/assistant 正文 + 工具调用摘要（reasoning 略）。
pub fn export_markdown(dir: &Path, id: &str) -> std::io::Result<String> {
    let session = load_meta(dir, id)?;
    let messages = load_messages_checked(dir, id)?;
    let mut out = format!("# {}\n\n- session: {}\n- directory: {}\n", session.title, session.id, session.directory);
    if let Some(parent_id) = &session.parent_id {
        let root_id = session.branch_root_id.as_deref().unwrap_or(parent_id);
        writeln!(&mut out, "- branch-root: {root_id}").expect("writing to String cannot fail");
        writeln!(&mut out, "- parent-session: {parent_id}").expect("writing to String cannot fail");
        if let Some(point) = &session.fork_point {
            let position = match point.position {
                ForkPosition::Before => "before",
                ForkPosition::After => "after",
            };
            writeln!(&mut out, "- fork-point: {position} message {} (index {})", point.message_id, point.message_index)
                .expect("writing to String cannot fail");
        }
        let kind = match session.fork_kind.unwrap_or(ForkKind::Manual) {
            ForkKind::Manual => "manual",
            ForkKind::Edit => "edit",
            ForkKind::Rerun => "rerun",
        };
        writeln!(&mut out, "- fork-kind: {kind}").expect("writing to String cannot fail");
        out.push_str("- workspace-state: shared-current\n");
    }
    out.push('\n');
    for m in &messages {
        let role = match m.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => continue,
        };
        let mut body = String::new();
        for p in &m.parts {
            match p {
                Part::Text { text } => {
                    body.push_str(text);
                    body.push('\n');
                }
                Part::ToolCall { name, input, output, .. } => {
                    let summary: String = output.chars().take(120).collect();
                    write!(&mut body, "\n> tool `{name}`: {input} -> {summary}\n").expect("writing to String cannot fail");
                }
                Part::Image { media_type, data } => {
                    // 不嵌 base64（数 MB 文本的 markdown 不可读）：占位注明类型与解码后近似大小
                    writeln!(&mut body, "[图片 {media_type}，约 {} KB]", data.len() * 3 / 4 / 1024).expect("writing to String cannot fail");
                }
                Part::Approval { command, decision, .. } => {
                    write!(&mut body, "\n> 审批 {decision}: {command}\n").expect("writing to String cannot fail");
                }
                Part::Reasoning { .. } | Part::Context { .. } | Part::ContextSources { .. } => {}
            }
        }
        if !body.trim().is_empty() {
            write!(&mut out, "\n## {role}\n\n{body}\n").expect("writing to String cannot fail");
        }
    }
    Ok(out)
}

/// 导出到指定路径（空则 ~/Downloads/kxen-<title>-<ts>.md），返回落盘路径。
pub fn export_to_file(dir: &Path, id: &str, out: Option<&Path>) -> std::io::Result<PathBuf> {
    let md = export_markdown(dir, id)?;
    let path = match out {
        Some(p) => p.to_path_buf(),
        None => {
            let session = load_meta(dir, id)?;
            let slug: String = session.title.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).take(40).collect();
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join("Downloads").join(format!("kxen-{slug}-{}.md", now_ms()))
        }
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, md)?;
    Ok(path)
}
