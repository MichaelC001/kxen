use kxen_core::composer_suggest::{
    LocalCandidate, LocalSignals, LocalSuggestInput, parse_llm_suggestions, rank_local, rank_semantic, recent_session_context,
    workspace_candidates,
};

fn candidate(path: &str, summary: &str) -> LocalCandidate {
    LocalCandidate { path: path.into(), summary: summary.into(), modified_unix: 0, sensitive: false }
}

#[test]
fn full_draft_and_history_drive_lexical_ranking() {
    let input = LocalSuggestInput {
        draft: "修复登录 token 刷新失败".into(),
        history: vec!["OAuth callback 会返回过期 credential".into()],
        selected_paths: vec![],
        signals: LocalSignals::default(),
        now_unix: 1_000,
        limit: 3,
    };
    let ranked = rank_local(
        &input,
        vec![
            candidate("src/auth/token_refresh.rs", "OAuth credential refresh and callback"),
            candidate("src/theme/colors.rs", "UI palette tokens"),
        ],
    );

    assert_eq!(ranked[0].path, "src/auth/token_refresh.rs");
    assert_eq!(ranked[0].source, "local");
    assert!(ranked[0].reason.contains("输入") || ranked[0].reason.contains("历史"));
}

#[test]
fn selected_sensitive_and_unrelated_candidates_are_excluded() {
    let input = LocalSuggestInput {
        draft: "更新 auth 配置".into(),
        history: vec![],
        selected_paths: vec!["src/auth/config.rs".into()],
        signals: LocalSignals::default(),
        now_unix: 1_000,
        limit: 5,
    };
    let mut secret = candidate(".env", "auth token secret");
    secret.sensitive = true;
    let ranked =
        rank_local(&input, vec![candidate("src/auth/config.rs", "auth config"), secret, candidate("README.md", "project overview")]);

    assert!(ranked.iter().all(|item| item.path != "src/auth/config.rs"));
    assert!(ranked.iter().all(|item| item.path != ".env"));
    assert!(ranked.iter().all(|item| item.path != "README.md"));
}

#[test]
fn selected_attachment_and_recency_can_recall_candidates_without_draft_overlap() {
    let input = LocalSuggestInput {
        draft: "继续处理".into(),
        history: vec![],
        selected_paths: vec!["src/auth/session.rs".into()],
        signals: LocalSignals::default(),
        now_unix: 10_000,
        limit: 5,
    };
    let mut related = candidate("src/auth/token.rs", "credential lifecycle");
    related.modified_unix = 1;
    let mut recent = candidate("docs/release.md", "publish checklist");
    recent.modified_unix = 9_900;

    let ranked = rank_local(&input, vec![related, recent, candidate("src/theme/colors.rs", "palette")]);

    assert!(ranked.iter().any(|item| item.path == "src/auth/token.rs" && item.reason.contains("上下文")));
    assert!(ranked.iter().any(|item| item.path == "docs/release.md" && item.reason.contains("最近修改")));
    assert!(ranked.iter().all(|item| item.path != "src/theme/colors.rs"));
}

#[test]
fn git_and_session_signals_can_recall_files_without_lexical_overlap() {
    let input = LocalSuggestInput {
        draft: "继续处理刚才的问题".into(),
        history: vec![],
        selected_paths: vec![],
        signals: LocalSignals {
            changed_paths: vec!["src/ws/rpc.rs".into()],
            involved_paths: vec!["src/core/session.rs".into()],
            context_paths: vec![],
        },
        now_unix: 1_000,
        limit: 5,
    };
    let ranked = rank_local(
        &input,
        vec![candidate("src/ws/rpc.rs", ""), candidate("src/core/session.rs", ""), candidate("src/theme/colors.rs", "")],
    );

    assert_eq!(ranked.len(), 2);
    assert!(ranked.iter().any(|item| item.path == "src/ws/rpc.rs" && item.reason.contains("Git")));
    assert!(ranked.iter().any(|item| item.path == "src/core/session.rs" && item.reason.contains("Session")));
}

#[test]
fn semantic_ranking_fuses_local_order_and_cosine() {
    let candidates = vec![candidate("src/local.rs", ""), candidate("src/semantic.rs", "")];
    let ranked = rank_semantic(&candidates, &[Some(0.1), Some(0.95)], 2);
    assert_eq!(ranked[0].path, "src/semantic.rs");
    assert_eq!(ranked[0].source, "semantic");
}

#[test]
fn llm_output_is_strictly_limited_to_local_candidate_ids() {
    let allowed = vec![candidate("src/auth.rs", "")];
    let output = r#"[
      {"kind":"file","candidate_id":"file:src/auth.rs","reason":"相关认证代码"},
      {"kind":"file","candidate_id":"file:/etc/passwd","reason":"越界"},
      {"kind":"insert_text","text":"请验证 token 刷新路径","reason":"下一步"}
    ]"#;
    let suggestions = parse_llm_suggestions(output, &allowed);
    assert_eq!(suggestions.len(), 2);
    assert_eq!(suggestions[0].path, "src/auth.rs");
    assert_eq!(suggestions[1].kind, "insert_text");
    assert_eq!(suggestions[1].source, "llm");
}

#[test]
fn workspace_index_respects_gitignore_sensitive_paths_trust_and_symlinks() {
    let root = std::env::temp_dir().join(format!("kxen-composer-index-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join(".gitignore"), "ignored.log\n").unwrap();
    std::fs::write(root.join("src/auth.rs"), "refresh OAuth credential token").unwrap();
    std::fs::write(root.join("ignored.log"), "private ignored content").unwrap();
    std::fs::write(root.join(".env"), "TOKEN=secret").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/passwd", root.join("src/outside-link")).unwrap();

    let untrusted = workspace_candidates(&root, false);
    assert!(untrusted.iter().any(|item| item.path == "src/auth.rs"));
    assert!(untrusted.iter().all(|item| !matches!(item.path.as_str(), ".env" | "ignored.log" | "src/outside-link")));
    assert!(untrusted.iter().all(|item| item.summary.is_empty()));

    let trusted = workspace_candidates(&root, true);
    assert!(trusted.iter().find(|item| item.path == "src/auth.rs").is_some_and(|item| item.summary.contains("OAuth credential")));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn recent_session_context_keeps_four_useful_text_messages_and_prior_paths() {
    use kxen_core::agent::context::ContextItem;
    use kxen_core::core::session::{Part, Role, new_message};
    let text = |role, value: &str| new_message("ses_one", role, vec![Part::Text { text: value.into() }]);
    let messages = vec![
        new_message(
            "ses_one",
            Role::User,
            vec![
                Part::Text { text: "first".into() },
                Part::ContextSources { items: vec![ContextItem::File { path: "src/old.rs".into() }] },
            ],
        ),
        text(Role::Assistant, "second"),
        text(Role::User, "third"),
        text(Role::Assistant, "fourth"),
        text(Role::User, "fifth"),
        text(Role::System, "internal system notice"),
    ];
    let (history, paths) = recent_session_context(&messages);
    assert_eq!(history, ["second", "third", "fourth", "fifth"]);
    assert_eq!(paths, ["src/old.rs"]);
}
