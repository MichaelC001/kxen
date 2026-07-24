//! SSRF 守卫：webfetch 与 context 的 web/image 抓取统一在此过检。
//! 威胁模型：agent 会被页面内容诱导抓取攻击者构造的 URL；只拦 scheme 时，
//! loopback、内网 RFC1918、云 metadata（169.254.169.254）都会被读进 prompt。

use std::net::IpAddr;

/// 重定向跳数上限：防 301 环，也给逐跳 DNS 检查封顶。
const MAX_REDIRECT_HOPS: u32 = 5;

/// 单个 IP 是否命中拒绝段。
/// v4：loopback / RFC1918 / link-local（169.254.0.0/16，含云 metadata）/ 0.0.0.0
/// v6：::1 / fc00::/7（ULA）/ fe80::/10（link-local）/ ::
pub fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
            {
                return true;
            }
            // v4-mapped（::ffff:a.b.c.d，NAT64/DNS64 会真实出现）按内嵌 v4 重判，
            // 否则 ::ffff:127.0.0.1 绕过上面的 v6 段检查
            v6.to_ipv4_mapped()
                .is_some_and(|v4| is_blocked_ip(&IpAddr::V4(v4)))
        }
    }
}

/// 解析 URL host 并 DNS 解析，全部返回 IP 都必须不在拒绝段；scheme 也在此收口。
/// 残余风险：检查与连接是两次解析，对抗性 DNS 可在间隔内换 IP；彻底钉住需要
/// 把解析结果 resolve override 到连接上，桌面 agent 场景暂接受该风险。
pub async fn check_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("invalid url: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        s => return Err(format!("scheme {s} not allowed (http/https only)")),
    }
    let host = parsed.host_str().ok_or("url has no host")?;
    let port = parsed.port_or_known_default().ok_or("url has no port")?;
    // v6 字面量 host_str 带方括号（[::1]），剥掉再按 IP 解析；
    // 字面 IP 直接判定：lookup_host 拼 "host:port" 对 v6 有歧义，且不该走 DNS
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return if is_blocked_ip(&ip) {
            Err(format!("{host} is a blocked address"))
        } else {
            Ok(())
        };
    }
    // 域名返回全部 A/AAAA 记录，任一命中拒绝段即拒
    let addrs: Vec<IpAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("dns resolve failed for {host}: {e}"))?
        .map(|sa| sa.ip())
        .collect();
    if addrs.is_empty() {
        return Err(format!("dns resolve failed for {host}: no address"));
    }
    if let Some(bad) = addrs.iter().find(|ip| is_blocked_ip(ip)) {
        return Err(format!("{host} resolves to blocked address {bad}"));
    }
    Ok(())
}

/// 手动跟随重定向的 GET，每跳重新 check_url。
/// client 必须是 redirect::Policy::none()：reqwest 自动跟随发生在守卫之外，等于没检。
pub async fn get_guarded(client: &reqwest::Client, url: &str) -> Result<reqwest::Response, String> {
    let mut current = reqwest::Url::parse(url).map_err(|e| format!("invalid url: {e}"))?;
    for _ in 0..=MAX_REDIRECT_HOPS {
        check_url(current.as_str()).await?;
        let resp = client
            .get(current.clone())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_redirection() {
            return Ok(resp);
        }
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| format!("http {} redirect without location", resp.status()))?;
        // Location 可能是相对路径（/next、//other/x），join 按 RFC3986 解析
        current = current
            .join(location)
            .map_err(|e| format!("bad redirect location {location}: {e}"))?;
    }
    Err(format!("too many redirects (>{MAX_REDIRECT_HOPS})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn blocks_loopback_private_linklocal_v4() {
        for ip in [
            v4(127, 0, 0, 1),
            v4(127, 1, 2, 3), // loopback 是整个 127/8
            v4(10, 0, 0, 1),
            v4(172, 16, 0, 1),
            v4(172, 31, 255, 1),
            v4(192, 168, 0, 1),
            v4(169, 254, 169, 254), // 云 metadata
            v4(0, 0, 0, 0),
        ] {
            assert!(is_blocked_ip(&ip), "{ip} 应被拒绝");
        }
        for ip in [
            v4(8, 8, 8, 8),
            v4(172, 15, 0, 1),
            v4(172, 32, 0, 1),
            v4(1, 1, 1, 1),
        ] {
            assert!(!is_blocked_ip(&ip), "{ip} 应放行");
        }
    }

    #[test]
    fn blocks_v6_equivalents() {
        let blocked: [IpAddr; 7] = [
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            "fc00::1".parse().unwrap(), // ULA fc00::/7
            "fd12:3456::1".parse().unwrap(),
            "fe80::1".parse().unwrap(),          // link-local fe80::/10
            "::ffff:127.0.0.1".parse().unwrap(), // v4-mapped 回 v4 重判
            "::ffff:169.254.169.254".parse().unwrap(),
        ];
        for ip in blocked {
            assert!(is_blocked_ip(&ip), "{ip} 应被拒绝");
        }
        let allowed: [IpAddr; 2] = [
            "2606:4700:4700::1111".parse().unwrap(),
            "::ffff:8.8.8.8".parse().unwrap(),
        ];
        for ip in allowed {
            assert!(!is_blocked_ip(&ip), "{ip} 应放行");
        }
    }

    #[test]
    fn rejects_blocked_url_without_network() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // 字面 IP 不走 DNS，测试无网络依赖；十进制/十六进制写法由 URL 解析器归一成点分
        for url in [
            "http://127.0.0.1/",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]/",
            "http://2130706433/",
            "http://0x7f000001/",
        ] {
            let err = rt.block_on(check_url(url)).unwrap_err();
            assert!(err.contains("blocked"), "{url} -> {err}");
        }
        let err = rt.block_on(check_url("ftp://example.com/")).unwrap_err();
        assert!(err.contains("scheme"), "{err}");
    }
}
