//! 通知环形缓冲落盘（data_dir/notifications.json，cap CAP 条，重启恢复）。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// 与通知中心一致的内存上限
pub const CAP: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Notice {
    at: u64,
    text: String,
}

fn store_file() -> PathBuf {
    crate::core::paths::data_dir().join("notifications.json")
}

/// 启动恢复：文件缺失/损坏一律空缓冲（通知非关键数据，不值得为排障阻塞启动）
pub fn load() -> VecDeque<(u64, String)> {
    load_from(&store_file())
}

/// 新通知从头部进，超 CAP 截尾（最新在前，与通知中心展示序一致）
pub fn push(buf: &mut VecDeque<(u64, String)>, at: u64, text: String) {
    buf.push_front((at, text));
    buf.truncate(CAP);
}

/// 原子写（tmp + rename）：崩溃窗口最多丢一轮通知，不留半截 JSON
pub fn persist(buf: &VecDeque<(u64, String)>) {
    persist_to(&store_file(), buf);
}

fn load_from(path: &Path) -> VecDeque<(u64, String)> {
    let Ok(text) = std::fs::read_to_string(path) else { return VecDeque::new() };
    let Ok(notes) = serde_json::from_str::<Vec<Notice>>(&text) else { return VecDeque::new() };
    let mut buf: VecDeque<(u64, String)> = notes.into_iter().map(|n| (n.at, n.text)).collect();
    buf.truncate(CAP);
    buf
}

fn persist_to(path: &Path, buf: &VecDeque<(u64, String)>) {
    let notes: Vec<Notice> = buf.iter().map(|(at, text)| Notice { at: *at, text: text.clone() }).collect();
    let Ok(json) = serde_json::to_string_pretty(&notes) else { return };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("kxen-notif-{tag}-{}.json", std::process::id()))
    }

    #[test]
    fn roundtrip_and_cap() {
        let path = tmp("rt");
        let mut buf = VecDeque::new();
        for i in 0..60 {
            push(&mut buf, i, format!("n{i}"));
        }
        assert_eq!(buf.len(), CAP, "内存侧 cap 必须生效");
        persist_to(&path, &buf);
        let loaded = load_from(&path);
        assert_eq!(loaded.len(), CAP);
        assert_eq!(loaded.front().unwrap().1, "n59", "最新一条在头部");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_or_missing_file_yields_empty() {
        let path = tmp("bad");
        assert!(load_from(&path).is_empty(), "缺失文件 = 空缓冲");
        std::fs::write(&path, "not json").unwrap();
        assert!(load_from(&path).is_empty(), "损坏文件 = 空缓冲，不许 panic");
        let _ = std::fs::remove_file(&path);
    }
}
