//! / 命令：builtin + 统一知识系统 kind=Command 条目（模板正文 + argument-hint + needs 懒加载）。

use crate::knowledge::{self, Kind};
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
    ("ultracode", "大任务模式：分解 -> workflow 并行实现 -> 集成验证", Some("<实现任务>")),
    ("ultraplan", "多角度规划模式：架构/调研/风险并行 -> 综合成稿", Some("<规划问题>")),
    ("ultrareview", "对抗性多镜审查：正确性/安全/性能/约定", Some("<路径或范围>")),
    ("doctor", "环境自检（订阅凭证/目录/配置）", None),
    ("clear", "清空当前会话（开启草稿态）", None),
    ("model", "切换当前模型", Some("<provider/model>")),
    ("abort", "中断当前生成", None),
];

/// command.list 数据源：builtin + custom（skills 由调用方拼）。
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
    out.extend(
        knowledge::scan(workdir)
            .into_iter()
            .filter(|e| e.kind == Kind::Command && e.enabled)
            .map(|e| CommandInfo {
                name: e.slug,
                description: e.description,
                kind: "custom",
                argument_hint: e.argument_hint,
            }),
    );
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 发送时展开自定义命令：$ARGUMENTS 模板 + needs 依赖懒加载注入。非自定义命令返回 None。
pub fn expand(workdir: &Path, name: &str, args: &str) -> Option<String> {
    let entry = knowledge::scan(workdir)
        .into_iter()
        .find(|e| e.kind == Kind::Command && e.enabled && e.slug == name)?;
    let content = crate::agent::skills::expand_args(&entry.content, args, &[]);
    let deps = knowledge::resolve_needs(workdir, &entry.needs);
    Some(format!("{content}\n{deps}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kxen-cmd-{tag}-{}", std::process::id()));
        let cmds = dir.join(".agents/commands");
        std::fs::create_dir_all(&cmds).unwrap();
        std::fs::write(cmds.join("review.md"), "---\ndescription: 审查指定路径\nargument-hint: <路径>\n---\n审查 $ARGUMENTS\n").unwrap();
        dir
    }

    #[test]
    fn builtin_and_custom() {
        let dir = fixture("list");
        let list = list(&dir);
        assert!(list.iter().any(|c| c.name == "write-goal" && c.kind == "builtin"));
        let custom = list.iter().find(|c| c.name == "review").unwrap();
        assert_eq!(custom.kind, "custom");
        assert_eq!(custom.description, "审查指定路径");
        assert_eq!(custom.argument_hint.as_deref(), Some("<路径>"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn expand_template_with_args() {
        let dir = fixture("expand");
        let out = expand(&dir, "review", "src/auth").unwrap();
        assert!(out.contains("审查 src/auth"));
        assert!(expand(&dir, "nonexistent", "").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
