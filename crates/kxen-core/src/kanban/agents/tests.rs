use super::*;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-agents-{tag}-{}-{nanos}", std::process::id()))
}

fn sample() -> AgentDefinition {
    AgentDefinition {
        name: "qa-verifier".into(),
        role: "review".into(),
        model: "auto".into(),
        permission_profile: "readonly+test".into(),
        tools: None,
        prompt: "You verify. Report PASS/FAIL with evidence.".into(),
    }
}

fn custom_sample() -> AgentDefinition {
    AgentDefinition {
        name: "go-editor".into(),
        role: "execution".into(),
        model: "auto".into(),
        permission_profile: "custom".into(),
        tools: Some(vec!["read".into(), "glob".into(), "grep".into(), "edit".into(), "write".into(), "exec".into(), "lsp".into()]),
        prompt: "You edit Go code. Declare the verdict when done.".into(),
    }
}

#[test]
fn parse_render_roundtrip() {
    let definition = parse(&to_markdown(&sample())).unwrap();
    assert_eq!(definition, sample());
}

#[test]
fn parse_rejects_malformed_frontmatter() {
    for (tag, text) in [
        ("no-header", "name: qa\nrole: r\nmodel: m\npermission_profile: readonly\nbody"),
        ("unterminated", "---\nname: qa\nrole: r\nmodel: m\npermission_profile: readonly\nbody"),
        ("unknown-field", "---\nname: qa\nrole: r\nmodel: m\npermission_profile: readonly\nextra: x\n---\nbody"),
        ("missing-field", "---\nname: qa\nrole: r\npermission_profile: readonly\n---\nbody"),
        ("duplicate", "---\nname: qa\nname: qa\nrole: r\nmodel: m\npermission_profile: readonly\n---\nbody"),
        ("empty-value", "---\nname: qa\nrole: \nmodel: m\npermission_profile: readonly\n---\nbody"),
        ("not-kv", "---\nname qa\nrole: r\nmodel: m\npermission_profile: readonly\n---\nbody"),
        ("empty-body", "---\nname: qa\nrole: r\nmodel: m\npermission_profile: readonly\n---\n  \n"),
        ("bad-profile", "---\nname: qa\nrole: r\nmodel: m\npermission_profile: root\n---\nbody"),
        ("bad-name", "---\nname: ../escape\nrole: r\nmodel: m\npermission_profile: readonly\n---\nbody"),
    ] {
        assert!(parse(text).is_err(), "{tag} 必须拒绝: {text:?}");
    }
}

#[test]
fn save_load_roundtrip_and_name_mismatch_rejected() {
    let workspace = temp("roundtrip");
    save(&workspace, &sample()).unwrap();
    let loaded = load(&workspace, "qa-verifier").unwrap();
    assert_eq!(loaded, sample());
    // 文件名与 frontmatter name 不一致：fail-closed
    let mut renamed = sample();
    renamed.name = "other-name".into();
    std::fs::write(agents_dir(&workspace).join("qa-verifier.md"), to_markdown(&renamed)).unwrap();
    assert!(matches!(load(&workspace, "qa-verifier"), Err(KanbanError::InvalidAgentDef(_))));
    // 穿越名与缺失文件
    assert!(matches!(load(&workspace, "../escape"), Err(KanbanError::InvalidId(_))));
    assert!(matches!(load(&workspace, "missing"), Err(KanbanError::Log(_))));
    // 坏定义不允许落盘（写路径与读路径同守卫）
    let mut bad = sample();
    bad.permission_profile = "root".into();
    assert!(save(&workspace, &bad).is_err());
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn profile_tools_mapping() {
    let mut definition = sample();
    definition.permission_profile = "readonly".into();
    let readonly = resolve_allowed_tools(&definition).unwrap().unwrap();
    for tool in ["edit", "write", "delete", "exec"] {
        assert!(!readonly.iter().any(|name| name == tool), "readonly 不得含 {tool}");
    }
    definition.permission_profile = "readonly+test".into();
    let with_test = resolve_allowed_tools(&definition).unwrap().unwrap();
    assert!(with_test.iter().any(|name| name == "exec"), "readonly+test 必须能跑验证命令");
    assert!(!with_test.iter().any(|name| name == "write"));
    definition.permission_profile = "full".into();
    assert!(resolve_allowed_tools(&definition).unwrap().is_none(), "full = 全部常驻工具");
    definition.permission_profile = "root".into();
    assert!(resolve_allowed_tools(&definition).is_err(), "未知 profile 拒绝而非兜底");
    // custom 直接取定义里已校验的工具集
    let custom = resolve_allowed_tools(&custom_sample()).unwrap().unwrap();
    assert!(custom.iter().any(|name| name == "lsp"), "custom 白名单可含 deferred 工具");
    assert!(!custom.iter().any(|name| name == "delete"));
}

#[test]
fn custom_roundtrip_and_profile_tools_rules() {
    // custom 往返恒等（tools 行进 frontmatter 再解析回来）
    assert_eq!(parse(&to_markdown(&custom_sample())).unwrap(), custom_sample());
    // custom 缺 tools 拒
    assert!(parse("---\nname: qa\nrole: r\nmodel: m\npermission_profile: custom\n---\nbody").is_err());
    // 固定三档带 tools 拒（权限语义单一来源）
    for profile in ["readonly", "readonly+test", "full"] {
        let text = format!("---\nname: qa\nrole: r\nmodel: m\npermission_profile: {profile}\ntools: read\n---\nbody");
        assert!(parse(&text).is_err(), "{profile} 不得自带 tools");
    }
}

#[test]
fn custom_tools_closed_set_fail_closed() {
    let parse_custom =
        |tools: &str| parse(&format!("---\nname: qa\nrole: r\nmodel: m\npermission_profile: custom\ntools: {tools}\n---\nbody"));
    // 闭集之外一律拒绝：跨 run 派发面、远端自报、门控不适用、列上下文不可用、未知名（含大写变体）
    for bad in [
        "agent",
        "workflow",
        "kanban_agent_create",
        "mcp__x",
        "schedule",
        "tool_search",
        "browser",
        "team",
        "todo",
        "skill",
        "unknown",
        "READ",
    ] {
        assert!(parse_custom(bad).is_err(), "{bad} 必须拒绝");
    }
    // 空项与重复项拒
    assert!(parse_custom("read,,grep").is_err(), "空项必须拒绝");
    assert!(parse_custom("read,read").is_err(), "重复项必须拒绝");
    // 逗号分隔带空白是合法输入
    assert!(parse_custom("read, glob , grep").is_ok());
    // 非法定义不落盘（save 与 parse 同守卫）
    let workspace = temp("badcustom");
    let mut bad = custom_sample();
    bad.tools = Some(vec!["read".into(), "kanban_agent_create".into()]);
    assert!(save(&workspace, &bad).is_err());
    assert!(!agents_dir(&workspace).join("go-editor.md").exists(), "非法定义不得落盘");
    std::fs::remove_dir_all(workspace).ok();
}
