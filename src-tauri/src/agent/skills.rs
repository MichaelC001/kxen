//! skills 系统：.kxen/skills + ~/.kxen/skills + .agents/skills 扫描（扁平 .md 与目录型 SKILL.md 并存）。
//! 规范依据 docs/research/2026-07-21-agent-ux.md §2：name/description 必填、清单 250 字符截断、
//! 递归深度 cap 3、$ARGUMENTS 展开、同 args 禁止重调、项目覆盖用户同名 first-wins。

use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 8;
const LISTING_DESC_MAX: usize = 250;
pub const SKILL_RECURSION_CAP: u32 = 3;

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub when_to_use: Option<String>,
    pub arguments: Vec<String>,
    pub disable_model_invocation: bool,
    pub user_invocable: bool,
    pub dir: PathBuf,
    pub content: String,
}

/// 项目优先于用户目录（同名 first-wins）。
pub fn scan(workdir: &Path) -> Vec<Skill> {
    let mut roots: Vec<PathBuf> = vec![workdir.join(".kxen/skills"), workdir.join(".agents/skills")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".kxen/skills"));
        roots.push(home.join(".agents/skills"));
    }
    let mut skills: Vec<Skill> = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let mut stack = vec![(root.clone(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let skill_md = path.join("SKILL.md");
                    if skill_md.exists() {
                        if let Ok(text) = std::fs::read_to_string(&skill_md) {
                            if let Some(skill) = parse(&path, &text) {
                                push_unique(&mut skills, skill);
                            }
                        }
                    } else {
                        stack.push((path, depth + 1));
                    }
                } else if path.extension().is_some_and(|x| x == "md") && path.file_name().is_some_and(|n| n != "SKILL.md") {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        if let Some(skill) = parse(&path, &text) {
                            push_unique(&mut skills, skill);
                        }
                    }
                }
            }
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

fn push_unique(skills: &mut Vec<Skill>, skill: Skill) {
    if !skills.iter().any(|s| s.name == skill.name) {
        skills.push(skill);
    }
}

/// 宽松 frontmatter 解析（未知字段不致命；name/description 缺失则跳过）。
fn parse(path: &Path, text: &str) -> Option<Skill> {
    let mut name = String::new();
    let mut description = String::new();
    let mut when_to_use = None;
    let mut arguments = Vec::new();
    let mut disable_model_invocation = false;
    let mut user_invocable = true;
    let mut content = text;

    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                let Some((key, value)) = line.split_once(':') else { continue };
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                match key {
                    "name" => name = value.chars().take(64).collect(),
                    "description" => description = value.chars().take(1024).collect(),
                    "when_to_use" | "when-to-use" => when_to_use = Some(value.to_string()),
                    "arguments" => arguments = value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                    "disable-model-invocation" | "disable_model_invocation" => disable_model_invocation = matches!(value, "true" | "yes" | "1"),
                    "user-invocable" | "user_invocable" => user_invocable = !matches!(value, "false" | "no" | "0"),
                    _ => {}
                }
            }
            content = rest[end + 4..].trim_start_matches('\n');
        }
    }
    // 目录型缺 name 用目录名兜底（开放标准要求同名）；扁平 .md 用文件名
    if name.is_empty() {
        name = path.file_stem()?.to_string_lossy().into_owned();
        if name == "SKILL" {
            name = path.parent()?.file_name()?.to_string_lossy().into_owned();
        }
    }
    if description.is_empty() {
        return None;
    }
    Some(Skill {
        name,
        description,
        when_to_use,
        arguments,
        disable_model_invocation,
        user_invocable,
        dir: path.to_path_buf(),
        content: content.to_string(),
    })
}

/// 清单段（system prompt 注入）：name + description(250) + when_to_use。
pub fn render_listing(workdir: &Path) -> Option<String> {
    let skills = scan(workdir);
    if skills.is_empty() {
        return None;
    }
    let mut out = String::from("\n\n## Available skills\n\nLoad a skill with the skill tool when the task matches its description. Do not reload one already loaded with identical args.\n");
    for s in &skills {
        let desc: String = s.description.chars().take(LISTING_DESC_MAX).collect();
        out.push_str(&format!("\n- {}: {}", s.name, desc));
        if let Some(w) = &s.when_to_use {
            out.push_str(&format!(" (use when: {w})"));
        }
    }
    Some(out)
}

/// 装载：$ARGUMENTS 展开 + 统一包装（调研 §2 形态）。
pub fn render_loaded(skill: &Skill, args: &str, trigger: &str) -> String {
    let mut content = skill.content.clone();
    let raw_args: Vec<&str> = args.split_whitespace().collect();
    if content.contains("$ARGUMENTS") {
        content = content.replace("$ARGUMENTS", args);
    }
    for (i, arg) in raw_args.iter().enumerate() {
        content = content.replace(&format!("${}", i + 1), arg);
        content = content.replace(&format!("$ARGUMENTS[{i}]"), arg);
    }
    if !content.contains("$") || (!skill.content.contains("$ARGUMENTS") && !skill.arguments.is_empty()) {
        // 无占位符：尾部追加（kimi-code 同款行为）
        if !args.is_empty() && !skill.content.contains("$ARGUMENTS") {
            content.push_str(&format!("\nARGUMENTS: {args}"));
        }
    }
    format!("<kxen-skill-loaded name=\"{}\" trigger=\"{trigger}\" dir=\"{}\" args=\"{args}\">\n{content}\n</kxen-skill-loaded>", skill.name, skill.dir.display())
}

pub fn find(workdir: &Path, name: &str) -> Option<Skill> {
    scan(workdir).into_iter().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kxen-skills-{tag}-{}", std::process::id()));
        let flat = dir.join(".kxen/skills");
        std::fs::create_dir_all(&flat).unwrap();
        std::fs::write(
            flat.join("commit.md"),
            "---\nname: commit\ndescription: Conventional Commits 提交助手\nwhen_to_use: 提交代码时\n---\n请按规范提交：$ARGUMENTS\n",
        )
        .unwrap();
        let nested = dir.join(".agents/skills/review");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("SKILL.md"), "---\nname: review\ndescription: 对抗性审查\n---\n审查 $1 的改动。\n").unwrap();
        dir
    }

    #[test]
    fn scan_flat_and_nested() {
        let dir = fixture("scan");
        let skills = scan(&dir);
        assert_eq!(skills.len(), 2);
        assert!(skills.iter().any(|s| s.name == "commit"));
        assert!(skills.iter().any(|s| s.name == "review"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn listing_truncates_and_includes_when_to_use() {
        let dir = fixture("listing");
        let listing = render_listing(&dir).unwrap();
        assert!(listing.contains("commit: Conventional Commits 提交助手"));
        assert!(listing.contains("use when: 提交代码时"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn arguments_expansion() {
        let dir = fixture("args");
        let skill = find(&dir, "commit").unwrap();
        let loaded = render_loaded(&skill, "fix login bug", "user");
        assert!(loaded.contains("请按规范提交：fix login bug"));
        assert!(loaded.contains("name=\"commit\""));
        assert!(loaded.contains("trigger=\"user\""));

        let review = find(&dir, "review").unwrap();
        let loaded2 = render_loaded(&review, "src/auth", "model");
        assert!(loaded2.contains("审查 src/auth 的改动"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_description_is_skipped() {
        let dir = std::env::temp_dir().join(format!("kxen-skills-bad-{}", std::process::id()));
        let flat = dir.join(".kxen/skills");
        std::fs::create_dir_all(&flat).unwrap();
        std::fs::write(flat.join("nodesc.md"), "---\nname: nodesc\n---\nbody\n").unwrap();
        assert!(scan(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
