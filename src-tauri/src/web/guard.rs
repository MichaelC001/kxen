//! WS 握手检查（upgrade 之前）：token + Origin + Host 白名单。
//! Jupyter 范式：本机端口不裸奔，token 是唯一防线；Host 白名单防 DNS rebinding。

/// 握手 query 里的 ?token= 提取（token 是 hex，无需 URL decode）。
fn token_from_query(query: &str) -> Option<&str> {
    query.split('&').find_map(|pair| pair.strip_prefix("token="))
}

/// Origin 检查：无 Origin（原生 webview/非浏览器客户端可能不带）与 Tauri webview / 本地 dev 前端放行；
/// 其余要求 Origin 的 authority（host[:port]）与 Host header 一致——浏览器同源场景，覆盖 tailscale/局域网。
fn origin_allowed(origin: Option<&str>, host_header: Option<&str>) -> bool {
    match origin {
        None => true,
        Some("tauri://localhost" | "http://tauri.localhost" | "http://localhost:7823") => true,
        Some(o) => host_header.is_some_and(|host| origin_authority(o).is_some_and(|authority| authority.eq_ignore_ascii_case(host.trim()))),
    }
}

/// Origin 的 authority 段（scheme 之后、首个 / 之前；Origin 规范本无路径，容错处理）。
fn origin_authority(origin: &str) -> Option<&str> {
    let after_scheme = origin.split("://").nth(1)?;
    let authority = after_scheme.split('/').next()?;
    if authority.is_empty() { None } else { Some(authority) }
}

/// Host header 白名单：localhost / 127.0.0.1 / [::1]（含端口形态）+ 实际 bind 地址
/// + 调用方追加项（kxen `--allow-host`，tailscale 域名场景；桌面恒空）。
fn host_allowed(host_header: Option<&str>, bind_host: &str, extra_hosts: &[String]) -> bool {
    let Some(host) = host_header else { return false };
    let hostname = strip_port(host.trim());
    matches!(hostname, "localhost" | "127.0.0.1" | "::1")
        || hostname.eq_ignore_ascii_case(bind_host)
        || extra_hosts.iter().any(|allowed| hostname.eq_ignore_ascii_case(allowed.trim()))
}

/// Host header 去端口：支持 `host:port`、`[v6]:port`、裸 `[v6]` 形态。
fn strip_port(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    host.split(':').next().unwrap_or(host)
}

/// 三道检查合成：token 不符、Origin 不可信、Host 不在白名单，任一失败即 403。
pub(super) fn handshake_allowed(
    query: Option<&str>,
    origin: Option<&str>,
    host: Option<&str>,
    expected_token: &str,
    bind_host: &str,
    extra_hosts: &[String],
) -> bool {
    let token_ok = query.and_then(token_from_query).is_some_and(|token| token == expected_token);
    token_ok && origin_allowed(origin, host) && host_allowed(host, bind_host, extra_hosts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_from_query_extracts() {
        assert_eq!(token_from_query("token=abc123"), Some("abc123"));
        assert_eq!(token_from_query("x=1&token=zz&y=2"), Some("zz"));
        assert_eq!(token_from_query(""), None);
        assert_eq!(token_from_query("x=1"), None);
    }

    #[test]
    fn origin_whitelist() {
        // 无 Origin（非浏览器客户端/原生 webview）放行
        assert!(origin_allowed(None, None));
        // Tauri webview 与本地 dev 前端白名单（无 Host 也放行）
        for ok in ["tauri://localhost", "http://tauri.localhost", "http://localhost:7823"] {
            assert!(origin_allowed(Some(ok), None), "{ok} 应放行");
        }
        for bad in ["http://evil.com", "https://localhost:7823", "http://localhost:1", "null"] {
            assert!(!origin_allowed(Some(bad), None), "{bad} 应拒绝");
        }
    }

    #[test]
    fn origin_matches_host_for_same_origin_browser() {
        // 浏览器同源：Origin authority == Host（tailscale/局域网任意 hostname 均可）
        assert!(origin_allowed(Some("http://machine.tailnet:7824"), Some("machine.tailnet:7824")));
        assert!(origin_allowed(Some("http://192.168.1.5:7824"), Some("192.168.1.5:7824")));
        assert!(!origin_allowed(Some("http://evil.com:7824"), Some("machine.tailnet:7824")));
        assert!(!origin_allowed(Some("http://machine.tailnet:7824"), None));
    }

    #[test]
    fn host_whitelist() {
        let none: &[String] = &[];
        for ok in ["localhost", "localhost:7824", "127.0.0.1", "127.0.0.1:7824", "[::1]", "[::1]:7824"] {
            assert!(host_allowed(Some(ok), "127.0.0.1", none), "{ok} 应放行");
        }
        // 实际 bind 地址（含端口形态）放行
        assert!(host_allowed(Some("10.0.0.2"), "10.0.0.2", none));
        assert!(host_allowed(Some("10.0.0.2:7824"), "10.0.0.2", none));
        for bad in ["evil.com", "127.0.0.1.evil.com", "localhost.evil.com:7824", "::2", "10.0.0.3:7824"] {
            assert!(!host_allowed(Some(bad), "127.0.0.1", none), "{bad} 应拒绝");
        }
        assert!(!host_allowed(None, "127.0.0.1", none));
    }

    #[test]
    fn extra_hosts_extend_whitelist() {
        let extra = vec!["machine.tailnet".to_string()];
        assert!(host_allowed(Some("machine.tailnet"), "127.0.0.1", &extra));
        assert!(host_allowed(Some("machine.tailnet:7824"), "127.0.0.1", &extra));
        assert!(host_allowed(Some("MACHINE.tailnet:7824"), "127.0.0.1", &extra));
        // 追加项不放宽其余拒绝规则
        assert!(!host_allowed(Some("machine.tailnet.evil.com"), "127.0.0.1", &extra));
        assert!(!host_allowed(Some("other.tailnet"), "127.0.0.1", &extra));
    }

    #[test]
    fn handshake_requires_all_three() {
        let none: &[String] = &[];
        let token = "a".repeat(64);
        let q = format!("token={token}");
        assert!(handshake_allowed(Some(&q), None, Some("127.0.0.1:7824"), &token, "127.0.0.1", none));
        assert!(handshake_allowed(Some(&q), Some("tauri://localhost"), Some("localhost:7824"), &token, "127.0.0.1", none));
        // token 错 / 缺 / Origin 恶 / Host 恶 各自独立拒绝
        assert!(!handshake_allowed(Some("token=wrong"), None, Some("127.0.0.1:7824"), &token, "127.0.0.1", none));
        assert!(!handshake_allowed(None, None, Some("127.0.0.1:7824"), &token, "127.0.0.1", none));
        assert!(!handshake_allowed(Some(&q), Some("http://evil.com"), Some("127.0.0.1:7824"), &token, "127.0.0.1", none));
        assert!(!handshake_allowed(Some(&q), None, Some("evil.com"), &token, "127.0.0.1", none));
        // --allow-host 追加的 Host 放行（浏览器同源 Origin 随之满足）
        let extra = vec!["machine.tailnet".to_string()];
        assert!(handshake_allowed(Some(&q), None, Some("machine.tailnet:7824"), &token, "127.0.0.1", &extra));
        assert!(handshake_allowed(
            Some(&q),
            Some("http://machine.tailnet:7824"),
            Some("machine.tailnet:7824"),
            &token,
            "127.0.0.1",
            &extra
        ));
    }
}
