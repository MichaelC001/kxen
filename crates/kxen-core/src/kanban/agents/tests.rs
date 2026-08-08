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
        prompt: "You verify. Report PASS/FAIL with evidence.".into(),
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
    let readonly = profile_tools("readonly").unwrap().unwrap();
    for tool in ["edit", "write", "delete", "exec"] {
        assert!(!readonly.contains(&tool), "readonly 不得含 {tool}");
    }
    let with_test = profile_tools("readonly+test").unwrap().unwrap();
    assert!(with_test.contains(&"exec"), "readonly+test 必须能跑验证命令");
    assert!(!with_test.contains(&"write"));
    assert!(profile_tools("full").unwrap().is_none(), "full = 全部常驻工具");
    assert!(profile_tools("root").is_none(), "未知 profile 拒绝而非兜底");
}
