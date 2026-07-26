//! websearch 工具：DuckDuckGo HTML 端点检索（免 key）-> 标题/链接/摘要（deferred 工具）。

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const MAX_RESULTS: usize = 8;

/// 单例 client（UA/timeout 定制，与 LLM 共享 client 配置不同，自建池复用）。
fn http() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(TIMEOUT)
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) kxen/0.1")
                .build()
                .expect("websearch http client")
        })
        .clone()
}

#[derive(Debug, PartialEq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub async fn search(query: &str) -> Result<Vec<SearchHit>, String> {
    if query.trim().is_empty() {
        return Err("empty query".into());
    }
    let client = http();
    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencode(query));
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("http {}", resp.status()));
    }
    let body = resp.text().await.map_err(|e| e.to_string())?;
    Ok(parse_results(&body))
}

/// DDG HTML 结果解析：result__a（链接）+ result__snippet（摘要），uddg= 参数取真实 URL。
fn parse_results(html: &str) -> Vec<SearchHit> {
    static RE_LINK: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#"(?s)<a[^>]*class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#).unwrap());
    static RE_SNIPPET: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#"(?s)<a[^>]*class="result__snippet"[^>]*>(.*?)</a>"#).unwrap());
    static RE_TAG: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"<[^>]+>").unwrap());

    let clean = |s: &str| RE_TAG.replace_all(s, "").split_whitespace().collect::<Vec<_>>().join(" ");
    let snippets: Vec<String> = RE_SNIPPET.captures_iter(html).map(|c| clean(&c[1])).collect();
    RE_LINK
        .captures_iter(html)
        .take(MAX_RESULTS)
        .enumerate()
        .map(|(i, c)| SearchHit { title: clean(&c[2]), url: decode_uddg(&c[1]), snippet: snippets.get(i).cloned().unwrap_or_default() })
        .collect()
}

/// DDG 跳转链接解码：//duckduckgo.com/l/?uddg=<urlencoded> -> 原始 URL。
fn decode_uddg(href: &str) -> String {
    let Some(pos) = href.find("uddg=") else { return href.to_string() };
    let raw = &href[pos + 5..];
    let raw = raw.split('&').next().unwrap_or(raw);
    percent_decode(raw)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).replace('+', " ")
}

/// 最小 urlencode（query 参数用；reqwest 的 query()/form() 在被裁的 feature 里，手工拼）。
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn format_hits(hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return "no results".into();
    }
    hits.iter().enumerate().map(|(i, h)| format!("{}. {}\n   {}\n   {}", i + 1, h.title, h.url, h.snippet)).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ddg_html() {
        let html = r#"
        <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fdoc">Example Doc</a>
        <a class="result__snippet">a useful snippet here</a>
        <a class="result__a" href="https://direct.com/page">Direct Link</a>
        <a class="result__snippet">another snippet</a>
        "#;
        let hits = parse_results(html);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://example.com/doc");
        assert_eq!(hits[0].title, "Example Doc");
        assert_eq!(hits[0].snippet, "a useful snippet here");
        assert_eq!(hits[1].url, "https://direct.com/page");
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("https%3A%2F%2Fa.com%2Fx%3Fp%3D1"), "https://a.com/x?p=1");
    }
}
