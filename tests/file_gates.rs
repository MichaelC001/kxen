//! 工程门禁（cargo test 硬检查）：单文件 <= 350 行。
//! 覆盖 src/（rs）与 ui/src/（ts/tsx）；违规即测试失败。

use std::path::Path;

const MAX_LINES: usize = 350;

#[test]
fn file_size_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    visit(&root.join("src"), &["rs"], MAX_LINES, &mut offenders);
    visit(&root.join("ui/src"), &["ts", "tsx"], MAX_LINES, &mut offenders);
    assert!(offenders.is_empty(), "超 {MAX_LINES} 行门禁的文件:\n{}", offenders.join("\n"));
}

fn visit(dir: &Path, exts: &[&str], max: usize, offenders: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit(&path, exts, max, offenders);
        } else if path.extension().is_some_and(|e| exts.contains(&e.to_string_lossy().as_ref())) {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let lines = text.lines().count();
            if lines > max {
                offenders.push(format!("{path:?}: {lines} 行"));
            }
        }
    }
}
