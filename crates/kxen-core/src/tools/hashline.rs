//! hashline read：输出 LINE#HASH 锚点（ChunkFingerprint：行 hash + chunk 指纹）。
//! 单点编辑只使所在 chunk 的锚点失效，其余 chunk 的锚点保持稳定。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const CHUNK_SIZE: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub line: usize,
    pub hash: String,
}

impl std::fmt::Display for Anchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.line, self.hash)
    }
}

fn line_hash(line: &str) -> u64 {
    let mut h = DefaultHasher::new();
    line.trim().hash(&mut h);
    h.finish()
}

fn chunk_fingerprint(lines: &[&str], chunk_idx: usize) -> u64 {
    let mut h = DefaultHasher::new();
    let start = chunk_idx * CHUNK_SIZE;
    for line in lines.iter().skip(start).take(CHUNK_SIZE) {
        line.trim().hash(&mut h);
    }
    h.finish()
}

fn hex4(value: u64) -> String {
    format!("{:04x}", value & 0xFFFF)
}

pub fn generate_anchors(lines: &[&str]) -> Vec<Anchor> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let chunk_idx = i / CHUNK_SIZE;
            let mixed = line_hash(line) ^ chunk_fingerprint(lines, chunk_idx).rotate_left(17);
            Anchor { line: i + 1, hash: hex4(mixed) }
        })
        .collect()
}

/// read 分页输出：锚点基于 anchor_src 全文计算（与 edit 侧 generate_anchors 一致），
/// 仅渲染 display 的 [start, end) 窗口（0 基）。anchor_src 与 display 等长；
/// 分页若按窗口局部算锚点，行号与 chunk 指纹全错位，锚点编辑会全废。
pub fn render_anchored_window(anchor_src: &[&str], display: &[String], start: usize, end: usize) -> String {
    let anchors = generate_anchors(anchor_src);
    (start..end.min(display.len()))
        .map(|i| format!("{:>5}#{}  {}", anchors[i].line, anchors[i].hash, display[i]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// read 输出：锚点前缀行（`  42#a3f9  content`）。
pub fn render_anchored(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let anchors = generate_anchors(&lines);
    lines
        .iter()
        .zip(anchors.iter())
        .map(|(line, anchor)| format!("{:>5}#{}  {}", anchor.line, anchor.hash, line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_stable_and_chunk_scoped_invalidation() {
        // ChunkFingerprint 语义：编辑使所在 chunk 锚点失效，其他 chunk 免疫。
        let lines: Vec<&str> = (0..80).map(|i| if i == 0 { "head" } else { "body" }).collect();
        let a1 = generate_anchors(&lines);
        let a2 = generate_anchors(&lines);
        assert_eq!(a1, a2, "same input -> same anchors");

        // 行内容编辑（不增删行）：编辑行所在 chunk 锚点失效，其他 chunk 免疫
        let mut edited = lines.clone();
        edited[3] = "changed";
        let a3 = generate_anchors(&edited);
        assert_ne!(a3[3].hash, a1[3].hash, "edited line invalidated");
        let far_a1 = &a1[79];
        let far_a3 = &a3[79];
        assert_eq!(far_a1.hash, far_a3.hash, "far chunk immune to in-place edit");
    }

    #[test]
    fn render_format() {
        let out = render_anchored("fn main() {}\n");
        assert!(out.contains("#"), "should contain anchors");
        assert!(out.contains("fn main() {}"));
    }
}
