/// 按 '\n' 切行并记录每行是否带 '\r'（与 lines() 口径一致，但保留行尾信息）。
pub(crate) fn split_preserving_crlf(text: &str) -> (Vec<String>, Vec<bool>) {
    let mut lines = Vec::new();
    let mut crlf = Vec::new();
    for raw in text.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        match line.strip_suffix('\r') {
            Some(line) => {
                lines.push(line.to_string());
                crlf.push(true);
            }
            None => {
                lines.push(line.to_string());
                crlf.push(false);
            }
        }
    }
    (lines, crlf)
}

/// split_preserving_crlf 的逆操作：每行按记录补回 '\r'，行间统一 '\n'。
pub(crate) fn join_preserving_crlf(lines: &[String], crlf: &[bool], trailing_newline: bool) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(line);
        if crlf.get(index).copied().unwrap_or(false) {
            out.push('\r');
        }
    }
    if trailing_newline {
        out.push('\n');
    }
    out
}

/// 简单 diff：首个不同行起的 before/after（最多各 5 行）。
pub(crate) fn simple_diff(before: &[String], after: &[String]) -> String {
    let mut out = String::new();
    let common = before.iter().zip(after.iter()).take_while(|(a, b)| a == b).count();
    let before_tail = before.iter().skip(common).take(5);
    let after_tail = after.iter().skip(common).take(5);
    for line in before_tail {
        out.push_str(&format!("- {line}\n"));
    }
    for line in after_tail {
        out.push_str(&format!("+ {line}\n"));
    }
    out
}
