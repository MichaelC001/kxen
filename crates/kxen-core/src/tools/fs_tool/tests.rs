use super::*;
use crate::tools::hashline::generate_anchors;

// find_shifted 是私有函数，此测试留在体内；公开 API 测试见 tests/fs_tool_eval.rs
#[test]
fn huge_anchor_line_number_does_not_panic() {
    let lines = vec!["a", "b"];
    let anchors = generate_anchors(&lines);
    assert!(find_shifted(&anchors, &lines, usize::MAX, "dead", 20).is_none());
    assert!(fresh_around(&lines, usize::MAX, 3).is_empty());
}

#[test]
fn shifted_anchor_recovers() {
    let lines = vec!["a", "b", "c", "d"];
    let anchors = generate_anchors(&lines);
    let shifted = find_shifted(&anchors, &lines, 3, &anchors[2].hash, 5);
    assert_eq!(shifted, Some(3));
}

#[test]
fn edit_preserves_crlf_line_endings() {
    let dir = std::env::temp_dir().join(format!("kxen-crlf-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let cwd = dir.to_str().unwrap().to_string();

    // match 模式：未触及行的 CRLF 原样保留
    let file = dir.join("win.txt");
    std::fs::write(&file, "alpha\r\nbeta\r\ngamma\r\n").unwrap();
    let spec = EditSpec::Match { old_string: "beta".into(), new_string: "BETA".into(), expected_replacements: Some(1) };
    edit(&file, &spec, &FileTracker::default(), &cwd).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\r\nBETA\r\ngamma\r\n");

    // 锚点模式：整文件行尾保持 CRLF
    let text = std::fs::read_to_string(&file).unwrap();
    let orig: Vec<&str> = text.lines().collect();
    let anchors = generate_anchors(&orig);
    let spec = EditSpec::Anchors { edits: vec![AnchorEdit { anchor: format!("3#{}", anchors[2].hash), new_text: "GAMMA".into() }] };
    edit(&file, &spec, &FileTracker::default(), &cwd).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\r\nBETA\r\nGAMMA\r\n");

    // 混合行尾：逐行保留，LF 行不被升级成 CRLF
    let mixed = dir.join("mixed.txt");
    std::fs::write(&mixed, "one\ntwo\r\nthree").unwrap();
    let text = std::fs::read_to_string(&mixed).unwrap();
    let orig: Vec<&str> = text.lines().collect();
    let anchors = generate_anchors(&orig);
    let spec = EditSpec::Anchors { edits: vec![AnchorEdit { anchor: format!("1#{}", anchors[0].hash), new_text: "ONE".into() }] };
    edit(&mixed, &spec, &FileTracker::default(), &cwd).unwrap();
    assert_eq!(std::fs::read_to_string(&mixed).unwrap(), "ONE\ntwo\r\nthree");
    std::fs::remove_dir_all(&dir).ok();
}
