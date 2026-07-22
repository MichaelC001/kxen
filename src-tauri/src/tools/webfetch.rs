//! webfetch 工具：拉 URL -> 粗提取正文文本（deferred 工具，经 tool_search 挂载）。

const MAX_CHARS: usize = 50_000;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

pub async fn fetch_text(url: &str) -> Result<String, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("url must start with https:// or http://".into());
    }
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent("kxen/0.1 (+https://kxen.ai)")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("http {}", resp.status()));
    }
    let body = resp.text().await.map_err(|e| e.to_string())?;
    Ok(strip_html(&body))
}

/// 粗提取：去 script/style；块级标签换行、行内标签空格，再折叠空白。够用即可，不做 DOM 解析。
fn strip_html(html: &str) -> String {
    static RE_SCRIPT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?is)<(script|style)[^>]*>.*?</(script|style)>").unwrap());
    static RE_BLOCK: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)</?(p|div|h[1-6]|li|br|tr|section|article|header|footer|ul|ol|table|blockquote)[^>]*>").unwrap());
    static RE_TAG: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"<[^>]+>").unwrap());
    static RE_WS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\n{3,}").unwrap());

    let no_script = RE_SCRIPT.replace_all(html, " ");
    let blocked = RE_BLOCK.replace_all(&no_script, "\n");
    let no_tags = RE_TAG.replace_all(&blocked, " ");
    let mut out = String::with_capacity(no_tags.len().min(MAX_CHARS));
    for line in no_tags.lines() {
        let trimmed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !trimmed.is_empty() {
            out.push_str(&trimmed);
            out.push('\n');
        }
        if out.len() >= MAX_CHARS {
            break;
        }
    }
    RE_WS.replace_all(&out, "\n\n").chars().take(MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_scripts() {
        let html = "<html><head><style>body{color:red}</style><script>evil()</script></head><body><h1>Title</h1><p>Hello <b>world</b></p></body></html>";
        let text = strip_html(html);
        assert!(text.contains("Title"));
        assert!(text.contains("Hello world"));
        assert!(!text.contains("evil"));
        assert!(!text.contains("color"));
    }

    #[test]
    fn rejects_non_http() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let err = rt.block_on(fetch_text("file:///etc/passwd")).unwrap_err();
        assert!(err.contains("https://"));
    }
}
