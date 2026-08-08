use super::StatusEntry;

/// porcelain v1 行解析：前 2 列状态码 + 路径；重命名 "R  old -> new" 取新路径。
pub(super) fn parse_status_line(line: &str) -> Option<StatusEntry> {
    if line.len() <= 3 {
        return None;
    }
    let code = line[..2].trim().to_string();
    let raw = line[3..].rsplit(" -> ").next().unwrap_or(&line[3..]);
    Some(StatusEntry { path: unquote_porcelain(raw), status: code })
}

/// porcelain v1 对含特殊字符的路径加 C 风格双引号（\t、\"、\\，非 ASCII 字节为 \ooo 八进制），
/// 不去引号反转义会让 dock 改动清单显示带引号的假路径。
pub(super) fn unquote_porcelain(path: &str) -> String {
    let Some(inner) = path.strip_prefix('"').and_then(|p| p.strip_suffix('"')) else {
        return path.to_string();
    };
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        index += 1;
        match bytes.get(index) {
            Some(b'n') => {
                out.push(b'\n');
                index += 1;
            }
            Some(b't') => {
                out.push(b'\t');
                index += 1;
            }
            Some(b'\\') => {
                out.push(b'\\');
                index += 1;
            }
            Some(b'"') => {
                out.push(b'"');
                index += 1;
            }
            Some(digit @ b'0'..=b'7') => {
                let mut byte = digit - b'0';
                index += 1;
                for _ in 0..2 {
                    match bytes.get(index) {
                        Some(digit @ b'0'..=b'7') => {
                            byte = byte * 8 + (digit - b'0');
                            index += 1;
                        }
                        _ => break,
                    }
                }
                out.push(byte);
            }
            Some(other) => {
                out.push(*other);
                index += 1;
            }
            None => {}
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
