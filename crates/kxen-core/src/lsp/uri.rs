//! file URI 编解码：LSP 规范要求 percent encoding（空格、#、非 ASCII 等）；解码用于 store 键与展示。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// path -> file:// URI。保留 unreserved 与 '/'，其余字节按 UTF-8 %XX 编码。
pub fn encode(path: &Path) -> String {
    let path = path.to_string_lossy();
    let mut out = String::with_capacity("file://".len() + path.len());
    out.push_str("file://");
    for &b in path.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => out.push(b as char),
            _ => write!(out, "%{b:02X}").expect("writing to String cannot fail"),
        }
    }
    out
}

/// file:// URI -> path（percent decode；非 file URI、非法编码或非 UTF-8 -> None）。
pub fn decode(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let s = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(s, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_ascii_path_unchanged() {
        assert_eq!(encode(Path::new("/w/src/main.rs")), "file:///w/src/main.rs");
    }

    #[test]
    fn encodes_space_hash_and_non_ascii() {
        assert_eq!(encode(Path::new("/w/my dir/a#b.rs")), "file:///w/my%20dir/a%23b.rs");
        let encoded = encode(Path::new("/w/中文/文件.rs"));
        assert!(encoded.starts_with("file:///w/"), "{encoded}");
        assert!(!encoded.contains('中'), "{encoded}");
    }

    #[test]
    fn roundtrip() {
        for p in ["/w/src/main.rs", "/w/my dir/a#b.rs", "/w/中文/文件.rs", "/w/100% sure/x.go"] {
            let encoded = encode(Path::new(p));
            assert_eq!(decode(&encoded).as_deref(), Some(Path::new(p)), "roundtrip {p}");
        }
    }

    #[test]
    fn decode_rejects_non_file_and_bad_encoding() {
        assert!(decode("https://example.com/x").is_none());
        assert!(decode("file:///w/%zz").is_none());
        assert!(decode("file:///w/trailing%").is_none());
    }

    #[test]
    fn decode_plain_ascii() {
        assert_eq!(decode("file:///w/src/main.rs").as_deref(), Some(Path::new("/w/src/main.rs")));
    }
}
