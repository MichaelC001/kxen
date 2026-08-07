//! 附件授权清单与读取：注册/校验/会话清理、workspace rel 计算、文本/base64 分流、2MB cap。

use kxen_gui::core::attachment::{ATTACH_CAP, PickedFiles, media_type_for, read_attachment, rel_in_workspace};
use std::path::PathBuf;

/// macOS temp 是 /var 软链：返回 canonical 路径，rel/比对才有稳定前缀。
fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kxen-attach-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn picked_allow_check_and_drop_session() {
    let picked = PickedFiles::default();
    let a = PathBuf::from("/tmp/a.txt");
    let b = PathBuf::from("/tmp/b.txt");
    picked.allow("s1", a.clone());
    picked.allow("s1", b.clone());
    picked.allow("s2", b.clone());

    assert!(picked.is_allowed("s1", &a));
    assert!(!picked.is_allowed("s1", &PathBuf::from("/tmp/c.txt")), "未注册路径不得放行");
    assert!(!picked.is_allowed("s2", &a), "授权按 session 隔离");
    assert_eq!(picked.snapshot("s1").unwrap().len(), 2);
    assert!(picked.snapshot("s-none").is_none());

    // session_delete 清理链：删 s1 只清 s1，幂等
    picked.drop_session("s1");
    assert!(!picked.is_allowed("s1", &a));
    assert!(picked.is_allowed("s2", &b), "别的 session 不受影响");
    picked.drop_session("s1");
}

#[test]
fn rel_in_workspace_inside_and_outside() {
    let work = tmp_dir("rel");
    let inside = work.join("src").join("a.txt");
    std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
    std::fs::write(&inside, "x").unwrap();
    let canon = inside.canonicalize().unwrap();
    assert_eq!(rel_in_workspace(&canon, &work).as_deref(), Some("src/a.txt"));

    let outside_dir = tmp_dir("rel-out");
    let outside = outside_dir.join("b.txt");
    std::fs::write(&outside, "x").unwrap();
    let canon_out = outside.canonicalize().unwrap();
    assert_eq!(rel_in_workspace(&canon_out, &work), None, "工作区外不得折算 rel");

    std::fs::remove_dir_all(&work).ok();
    std::fs::remove_dir_all(&outside_dir).ok();
}

#[test]
fn read_attachment_text_and_binary() {
    let dir = tmp_dir("read");
    let text_file = dir.join("note.txt");
    std::fs::write(&text_file, "hello 附件").unwrap();
    let v = read_attachment(&text_file).unwrap();
    assert_eq!(v["kind"], "text");
    assert_eq!(v["text"], "hello 附件");

    // 非 utf8 字节 -> base64 通道，media_type 按扩展名
    let bin_file = dir.join("pic.png");
    std::fs::write(&bin_file, [0x89, 0x50, 0x4E, 0x47, 0xFF, 0xFE]).unwrap();
    let v = read_attachment(&bin_file).unwrap();
    assert_eq!(v["kind"], "base64");
    assert_eq!(v["media_type"], "image/png");
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(v["data"].as_str().unwrap()).unwrap();
    assert_eq!(decoded, [0x89, 0x50, 0x4E, 0x47, 0xFF, 0xFE]);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_attachment_caps_at_2mb() {
    let dir = tmp_dir("cap");
    let big = dir.join("big.txt");
    std::fs::write(&big, "x".repeat(ATTACH_CAP + 1)).unwrap();
    let err = read_attachment(&big).unwrap_err();
    assert!(err.contains("2MB"), "超 cap 必须拒读: {err}");
    // 恰好在 cap 内的文本放行
    let exact = dir.join("exact.txt");
    std::fs::write(&exact, "x".repeat(ATTACH_CAP)).unwrap();
    assert_eq!(read_attachment(&exact).unwrap()["kind"], "text");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn media_type_mapping() {
    for (name, want) in [
        ("a.png", "image/png"),
        ("a.jpg", "image/jpeg"),
        ("a.JPEG", "image/jpeg"),
        ("a.gif", "image/gif"),
        ("a.webp", "image/webp"),
        ("a.bmp", "image/bmp"),
        ("a.txt", "application/octet-stream"),
        ("noext", "application/octet-stream"),
    ] {
        assert_eq!(media_type_for(std::path::Path::new(name)), want, "{name}");
    }
}
