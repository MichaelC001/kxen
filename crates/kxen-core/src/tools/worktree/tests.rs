use super::porcelain::{parse_status_line, unquote_porcelain};
use super::*;

#[test]
fn unquote_porcelain_decodes_c_style_escapes() {
    assert_eq!(unquote_porcelain("plain.txt"), "plain.txt");
    assert_eq!(unquote_porcelain("with space.txt"), "with space.txt");
    assert_eq!(unquote_porcelain("\"with space.txt\""), "with space.txt");
    assert_eq!(unquote_porcelain("\"a\\tb.txt\""), "a\tb.txt");
    assert_eq!(unquote_porcelain("\"a\\\"b.txt\""), "a\"b.txt");
    assert_eq!(unquote_porcelain("\"a\\\\b.txt\""), "a\\b.txt");
    // 非 ASCII 字节是 \ooo 八进制（core.quotepath 默认开启）：\344\275\240 = 你
    assert_eq!(unquote_porcelain("\"\\344\\275\\240.txt\""), "你.txt");
}

#[test]
fn parse_status_line_handles_quoted_and_rename_paths() {
    let entry = parse_status_line(" M src/a.rs").unwrap();
    assert_eq!(entry.status, "M");
    assert_eq!(entry.path, "src/a.rs");

    let entry = parse_status_line("?? \"new file.txt\"").unwrap();
    assert_eq!(entry.status, "??");
    assert_eq!(entry.path, "new file.txt");

    let entry = parse_status_line("R  \"old name.txt\" -> \"new name.txt\"").unwrap();
    assert_eq!(entry.status, "R");
    assert_eq!(entry.path, "new name.txt");

    assert!(parse_status_line("").is_none());
    assert!(parse_status_line(" M").is_none());
}

#[tokio::test]
async fn create_rejects_leftover_directory_that_is_not_a_worktree() {
    let repo = std::env::temp_dir().join(format!("kxen-wt-{}", uuid::Uuid::new_v4()));
    let leftover = repo.join(".kxen").join("worktrees").join("demo");
    std::fs::create_dir_all(&leftover).unwrap();

    let error = create(&repo, "demo").await.unwrap_err();
    assert!(error.contains("not a git worktree"), "{error}");

    // worktree 的 .git 是指回主仓库 gitdir 的文件：补齐后按复用处理
    std::fs::write(leftover.join(".git"), "gitdir: /somewhere\n").unwrap();
    let info = create(&repo, "demo").await.unwrap();
    assert_eq!(info.branch, "kxen/demo");
    std::fs::remove_dir_all(&repo).ok();
}
