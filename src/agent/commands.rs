//! / 命令：内置命令 + 自定义模板（.kxen/commands/*.md + ~/.kxen/commands/*.md，文件名=命令名）。

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    pub kind: &'static str, // "builtin" | "custom" | "skill"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
}

const BUILTIN: &[(&str, &str, Option<&str>)] = &[
    ("write-goal", "交互式定义一个带完成判据的 goal", Some("<目标描述>")),
    ("doctor", "环境自检（订阅凭证/目录/配置）", None),
    ("clear", "清空当前会话（开启草稿态）", None),
    ("model", "切换当前模型", Some("<provider/model>")),
    ("abort", "中断当前生成", None),
];

/// 自定义模板：.kxen/commands/*.md（frontmatter description/argument-hint，正文 $ARGUMENTS 模板）。
fn scan_custom(workdir: &Path) -> Vec<CommandInfo> {
    let mut roots = vec![workdir.join(".kxen/commands")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".kxen/commands"));
    }
    let mut out: Vec<CommandInfo> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|x| x == "md") {
                continue;
            }
            let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else { continue };
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let (description, argument_hint) = parse_meta(&text);
            if out.iter().any(|c: &CommandInfo| c.name == name) {
                continue; // 项目覆盖用户（first-wins）
            }
            out.push(CommandInfo { name, description, kind: "custom", argument_hint });
        }
    }
    out
}

fn parse_meta(text: &str) -> (String, Option<String>) {
    let mut description = String::new();
    let mut hint = None;
    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                let Some((key, value)) = line.split_once(':') else { continue };
                match key.trim() {
                    "description" => description = value.trim().trim_matches('"').to_string(),
                    "argument-hint" | "argument_hint" => hint = Some(value.trim().trim_matches('"').to_string()),
                    _ => {}
                }
            }
        }
    }
    (description, hint)
}

/// command.list 数据源：builtin + custom + skills（skills 由调用方拼）。
pub fn list(workdir: &Path) -> Vec<CommandInfo> {
    let mut out: Vec<CommandInfo> = BUILTIN
        .iter()
        .map(|(name, desc, hint)| CommandInfo {
            name: name.to_string(),
            description: desc.to_string(),
            kind: "builtin",
            argument_hint: hint.map(String::from),
        })
        .collect();
    out.extend(scan_custom(workdir));
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_and_custom() {
        let dir = std::env::temp_dir().join(format!("kxen-cmd-{}", std::process::id()));
        let cmds = dir.join(".kxen/commands");
        std::fs::create_dir_all(&cmds).unwrap();
        std::fs::write(cmds.join("review.md"), "---\ndescription: 审查指定路径\nargument-hint: <路径>\n---\n审查 $ARGUMENTS\n").unwrap();
        let list = list(&dir);
        assert!(list.iter().any(|c| c.name == "write-goal" && c.kind == "builtin"));
        let custom = list.iter().find(|c| c.name == "review").unwrap();
        assert_eq!(custom.kind, "custom");
        assert_eq!(custom.description, "审查指定路径");
        assert_eq!(custom.argument_hint.as_deref(), Some("<路径>"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
