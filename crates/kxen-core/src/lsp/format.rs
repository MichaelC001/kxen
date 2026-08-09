//! LSP 结果紧凑格式化：与 diagnostics 同风格（单行条目），保护 agent context（cap 条数/长度）。

use super::uri;
use serde_json::Value;
use std::fmt::Write as _;

const MAX_LOCATIONS: usize = 50;
const MAX_SYMBOL_LINES: usize = 100;
const MAX_HOVER_CHARS: usize = 2000;

/// hover result -> 文本；空 -> "no hover info"。兼容 MarkupContent / MarkedString / 数组三种形态。
pub fn hover(result: &Value) -> String {
    let mut parts = Vec::new();
    collect_markup(result.get("contents").unwrap_or(&Value::Null), &mut parts);
    let mut text = parts.join("\n");
    trim_in_place(&mut text);
    if text.is_empty() {
        return "no hover info".into();
    }
    if text.chars().count() > MAX_HOVER_CHARS {
        let truncated: String = text.chars().take(MAX_HOVER_CHARS).collect();
        return format!("{truncated}\n... (truncated)");
    }
    text
}

fn collect_markup<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(arr) => arr.iter().for_each(|x| collect_markup(x, out)),
        Value::Object(_) => {
            if let Some(s) = v.get("value").and_then(Value::as_str) {
                out.push(s);
            }
        }
        _ => {}
    }
}

/// definition/references result -> `path:line:col` 每行一条（1-based）；空 -> none_msg。
/// 兼容 Location 与 LocationLink（targetUri/targetRange）。
pub fn locations(result: &Value, none_msg: &str) -> String {
    let items = match result {
        Value::Array(items) => items.as_slice(),
        Value::Null => &[],
        single => std::slice::from_ref(single),
    };
    if items.is_empty() {
        return none_msg.into();
    }
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i >= MAX_LOCATIONS {
            writeln!(out, "... ({} more)", items.len() - MAX_LOCATIONS).expect("writing to String cannot fail");
            break;
        }
        let (uri, range) = if item.get("targetUri").is_some() {
            (item.get("targetUri").and_then(Value::as_str), item.get("targetRange"))
        } else {
            (item.get("uri").and_then(Value::as_str), item.get("range"))
        };
        let Some(start) = range.and_then(|r| r.get("start")) else {
            continue;
        };
        let (Some(line), Some(col)) = (start.get("line").and_then(Value::as_u64), start.get("character").and_then(Value::as_u64)) else {
            continue;
        };
        let path = uri.and_then(uri::decode).map(|p| p.display().to_string()).unwrap_or_else(|| uri.unwrap_or("?").to_string());
        writeln!(out, "{path}:{}:{}", line + 1, col + 1).expect("writing to String cannot fail");
    }
    if out.is_empty() {
        none_msg.into()
    } else {
        trim_end_in_place(&mut out);
        out
    }
}

/// documentSymbol result -> 缩进树 `name (kind) line`（1-based）；空 -> "no symbols"。
/// 兼容层级 DocumentSymbol 与扁平 SymbolInformation 两种返回。
pub fn symbols(result: &Value) -> String {
    let Some(arr) = result.as_array() else {
        return "no symbols".into();
    };
    let mut out = String::new();
    let mut budget = MAX_SYMBOL_LINES;
    for sym in arr {
        render_symbol(sym, 0, &mut out, &mut budget);
    }
    if budget == 0 {
        out.push_str("... (truncated)\n");
    }
    if out.is_empty() {
        "no symbols".into()
    } else {
        trim_end_in_place(&mut out);
        out
    }
}

fn render_symbol(sym: &Value, depth: usize, out: &mut String, budget: &mut usize) {
    if *budget == 0 {
        return;
    }
    let Some(name) = sym.get("name").and_then(Value::as_str) else {
        return;
    };
    let kind = sym.get("kind").and_then(Value::as_u64).map(kind_str).unwrap_or("?");
    // SymbolInformation 的 range 在 location 下；DocumentSymbol 直接在 range 下
    let range = sym.get("range").or_else(|| sym.get("location").and_then(|l| l.get("range")));
    let line = range.and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(Value::as_u64);
    for _ in 0..depth {
        out.push_str("  ");
    }
    match line {
        Some(line) => writeln!(out, "{name} ({kind}) {}", line + 1),
        None => writeln!(out, "{name} ({kind}) ?"),
    }
    .expect("writing to String cannot fail");
    *budget -= 1;
    if let Some(children) = sym.get("children").and_then(Value::as_array) {
        for child in children {
            render_symbol(child, depth + 1, out, budget);
        }
    }
}

fn trim_in_place(text: &mut String) {
    let start = text.len() - text.trim_start().len();
    let end = text.trim_end().len();
    if start >= end {
        text.clear();
        return;
    }
    text.truncate(end);
    text.drain(..start);
}

fn trim_end_in_place(text: &mut String) {
    text.truncate(text.trim_end().len());
}

fn kind_str(kind: u64) -> &'static str {
    match kind {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        23 => "struct",
        26 => "typeparam",
        _ => "symbol",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hover_markup_content() {
        let r = json!({ "contents": { "kind": "markdown", "value": "```rust\nfn main()\n```" } });
        assert_eq!(hover(&r), "```rust\nfn main()\n```");
    }

    #[test]
    fn hover_marked_string_array_and_empty() {
        let r = json!({ "contents": [{ "language": "rust", "value": "fn main()" }, "entry point"] });
        assert_eq!(hover(&r), "fn main()\nentry point");
        assert_eq!(hover(&json!(null)), "no hover info");
        assert_eq!(hover(&json!({ "contents": { "kind": "plaintext", "value": "  " } })), "no hover info");
    }

    #[test]
    fn locations_single_and_array() {
        let single = json!({ "uri": "file:///w/src/main.rs", "range": { "start": { "line": 2, "character": 4 } } });
        assert_eq!(locations(&single, "none"), "/w/src/main.rs:3:5");
        let arr = json!([
            { "uri": "file:///w/a.rs", "range": { "start": { "line": 0, "character": 0 } } },
            { "uri": "file:///w/my%20dir/b.rs", "range": { "start": { "line": 9, "character": 1 } } },
        ]);
        assert_eq!(locations(&arr, "none"), "/w/a.rs:1:1\n/w/my dir/b.rs:10:2");
        assert_eq!(locations(&json!(null), "no references found"), "no references found");
        assert_eq!(locations(&json!([]), "no definition found"), "no definition found");
    }

    #[test]
    fn locations_location_link() {
        let link = json!({ "targetUri": "file:///w/lib.rs", "targetRange": { "start": { "line": 4, "character": 2 } } });
        assert_eq!(locations(&link, "none"), "/w/lib.rs:5:3");
    }

    #[test]
    fn symbols_hierarchical() {
        let r = json!([
            { "name": "main", "kind": 12, "range": { "start": { "line": 0 } } },
            { "name": "App", "kind": 23, "range": { "start": { "line": 5 } }, "children": [
                { "name": "run", "kind": 6, "range": { "start": { "line": 8 } } },
            ] },
        ]);
        assert_eq!(symbols(&r), "main (function) 1\nApp (struct) 6\n  run (method) 9");
    }

    #[test]
    fn symbols_flat_symbol_information() {
        let r = json!([{ "name": "init", "kind": 12, "location": { "uri": "file:///w/a.go", "range": { "start": { "line": 3 } } } }]);
        assert_eq!(symbols(&r), "init (function) 4");
        assert_eq!(symbols(&json!(null)), "no symbols");
        assert_eq!(symbols(&json!([])), "no symbols");
    }
}
