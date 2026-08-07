use super::*;

#[test]
fn parses_tavily_response() {
    let body = r#"{"results":[{"title":"T","url":"https://a.com","content":"snippet"}]}"#;
    let h = parse_tavily(body).expect("tavily json");
    assert_eq!(h, vec![SearchHit { title: "T".into(), url: "https://a.com".into(), snippet: "snippet".into() }]);
}

#[test]
fn parses_brave_response() {
    let body = r#"{"web":{"results":[{"title":"B","url":"https://b.com","description":"desc"},{"title":"N","url":"https://n.com"}]}}"#;
    let h = parse_brave(body).expect("brave json");
    assert_eq!(h.len(), 2);
    assert_eq!(h[0].snippet, "desc");
    assert_eq!(h[1].snippet, "", "description 缺省容忍");
}

#[test]
fn parses_exa_response() {
    let body = r#"{"results":[{"title":"E","url":"https://e.com","highlights":["h1","h2"]},{"url":"https://x.com"}]}"#;
    let h = parse_exa(body).expect("exa json");
    assert_eq!(h[0].snippet, "h1 … h2");
    assert_eq!(h[1].title, "https://x.com", "title 缺省回落 url");
}

#[test]
fn parses_jina_response() {
    let body = r#"{"code":200,"data":[{"title":"J","url":"https://j.com","description":"d"},{"title":"C","url":"https://c.com","content":"long content"}]}"#;
    let h = parse_jina(body).expect("jina json");
    assert_eq!(h[0].snippet, "d");
    assert_eq!(h[1].snippet, "long content", "无 description 截 content");
}

#[test]
fn parses_link_style_three_vendors() {
    let serper = r#"{"organic":[{"title":"S","link":"https://s.com","snippet":"ss"}]}"#;
    let serpapi = r#"{"organic_results":[{"title":"P","link":"https://p.com","snippet":"pp"}]}"#;
    let google = r#"{"items":[{"title":"G","link":"https://g.com","snippet":"gg"}]}"#;
    assert_eq!(parse_link_style(serper, "organic", "serper").unwrap()[0].url, "https://s.com");
    assert_eq!(parse_link_style(serpapi, "organic_results", "serpapi").unwrap()[0].url, "https://p.com");
    assert_eq!(parse_link_style(google, "items", "google").unwrap()[0].url, "https://g.com");
}

#[test]
fn parses_firecrawl_response() {
    let body = r#"{"success":true,"data":[{"title":"F","url":"https://f.com","description":"fd"}]}"#;
    let h = parse_firecrawl(body).expect("firecrawl json");
    assert_eq!(h[0].snippet, "fd");
}

#[test]
fn parses_youcom_response() {
    let body = r#"{"results":{"web":[{"title":"Y","url":"https://y.com","description":"yd","snippets":["s1","s2"]}]}}"#;
    let h = parse_youcom(body).expect("you json");
    assert_eq!(h[0].snippet, "s1", "snippets 首条优先于 description");
}

#[test]
fn parses_searxng_response() {
    let body = r#"{"results":[{"title":"X","url":"https://x.com","content":"xc"}]}"#;
    let h = parse_searxng(body).expect("searxng json");
    assert_eq!(h[0].snippet, "xc");
}
