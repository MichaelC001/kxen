//! .mcp.json 解析：双 scope（项目 <workdir>/.mcp.json 覆盖用户 ~/.config/kxen/mcp.json）。
//! server 两种形态：stdio（command）与 remote（url + transport http|sse）；
//! 顶层 toolPolicies 按 "server" 或 "server.tool" 键给 per-tool 放行/询问/拒绝。

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// per-tool 策略三态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicy {
    Allow,
    Ask,
    Deny,
}

impl ToolPolicy {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(Self::Allow),
            "ask" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// 策略表：键 "server"（整台 server 默认）或 "server.tool"（单工具覆盖）。
#[derive(Debug, Default, Clone)]
pub struct PolicySet {
    inner: HashMap<String, ToolPolicy>,
}

impl PolicySet {
    pub fn insert(&mut self, key: &str, policy: ToolPolicy) {
        self.inner.insert(key.to_string(), policy);
    }

    /// 匹配顺序 server.tool > server > 默认 Allow。
    /// 默认 Allow 而非 Ask：server 本身来自用户显式配置或已信任项目，
    /// 默认 ask 会给存量调用强塞弹窗。
    pub fn for_tool(&self, server: &str, tool: &str) -> ToolPolicy {
        self.inner.get(&format!("{server}.{tool}")).copied().or_else(|| self.inner.get(server).copied()).unwrap_or(ToolPolicy::Allow)
    }

    /// 项目覆盖用户：同键以后 extend 进来的为准。
    fn extend(&mut self, other: PolicySet) {
        self.inner.extend(other.inner);
    }
}

#[derive(Debug, Clone)]
pub enum ServerConfig {
    Stdio(StdioConfig),
    Remote(RemoteConfig),
}

#[derive(Debug, Clone)]
pub struct StdioConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

/// remote server 的 OAuth 2.0 授权配置（全可选；授权流实现见 mcp/oauth.rs）。
/// 无 client_id 时走 RFC 7591 动态注册；有 client_id 跳过注册。
#[derive(Debug, Clone, Default)]
pub struct OAuthConfig {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    /// 回调端口；缺省 :0 随机（固定端口被占时回退随机）
    pub callback_port: Option<u16>,
    /// scope 串（空格分隔），缺省不带 scope 参数
    pub scopes: Option<String>,
    /// 跳过 discovery 直指的 AS 元数据 URL
    pub auth_server_metadata_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RemoteConfig {
    pub name: String,
    pub url: String,
    pub transport: RemoteKind,
    pub headers: HashMap<String, String>,
    pub oauth: Option<OAuthConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteKind {
    Http,
    Sse,
}

impl RemoteKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

impl ServerConfig {
    pub fn name(&self) -> &str {
        match self {
            Self::Stdio(c) => &c.name,
            Self::Remote(c) => &c.name,
        }
    }

    pub fn transport_kind(&self) -> &'static str {
        match self {
            Self::Stdio(_) => "stdio",
            Self::Remote(c) => c.transport.as_str(),
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            Self::Stdio(_) => None,
            Self::Remote(c) => Some(&c.url),
        }
    }
}

#[derive(Debug, Deserialize)]
struct McpFile {
    #[serde(rename = "mcpServers", default)]
    servers: HashMap<String, ServerDef>,
    #[serde(rename = "toolPolicies", default)]
    policies: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ServerDef {
    /// Claude 生态惯用 "type"，本配置也收 "transport"；都缺省按 command/url 推断
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    transport: Option<String>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    url: Option<String>,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    oauth: Option<OAuthDef>,
}

/// .mcp.json 的 oauth 对象：键名 camelCase（clientId/clientSecret/callbackPort/scopes/authServerMetadataUrl）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthDef {
    client_id: Option<String>,
    client_secret: Option<String>,
    callback_port: Option<u16>,
    scopes: Option<String>,
    auth_server_metadata_url: Option<String>,
}

fn parse_server(name: String, def: ServerDef) -> Option<ServerConfig> {
    let kind = def.kind.as_deref().or(def.transport.as_deref());
    if let Some(url) = def.url {
        // scheme 配置期先挡：运行时才报比加载时 warn 难定位得多
        match url.split_once("://").map(|(s, _)| s) {
            Some("http") | Some("https") => {}
            _ => {
                tracing::warn!(name, url, "mcp remote url 仅支持 http/https，跳过");
                return None;
            }
        }
        let transport = match kind {
            // 缺省 http：streamable http 是现行标准形态，legacy sse 需显式声明
            None | Some("http") => RemoteKind::Http,
            Some("sse") => RemoteKind::Sse,
            Some(other) => {
                tracing::warn!(name, transport = other, "mcp remote transport 非法（http|sse），跳过");
                return None;
            }
        };
        return Some(ServerConfig::Remote(RemoteConfig {
            name,
            url,
            transport,
            headers: def.headers,
            oauth: def.oauth.map(|o| OAuthConfig {
                client_id: o.client_id,
                client_secret: o.client_secret,
                callback_port: o.callback_port,
                scopes: o.scopes,
                auth_server_metadata_url: o.auth_server_metadata_url,
            }),
        }));
    }
    if let Some(command) = def.command {
        if let Some(k) = kind
            && k != "stdio"
        {
            tracing::warn!(name, kind = k, "command server 的 type 只能是 stdio，跳过");
            return None;
        }
        return Some(ServerConfig::Stdio(StdioConfig { name, command, args: def.args, env: def.env }));
    }
    tracing::warn!(name, "mcp server 既无 command 也无 url，跳过");
    None
}

fn load_file(path: &Path) -> (Vec<ServerConfig>, PolicySet) {
    let Ok(text) = std::fs::read_to_string(path) else { return (vec![], PolicySet::default()) };
    let parsed: McpFile = match serde_json::from_str(&text) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "mcp.json parse failed");
            return (vec![], PolicySet::default());
        }
    };
    let servers = parsed.servers.into_iter().filter_map(|(name, def)| parse_server(name, def)).collect();
    let mut policies = PolicySet::default();
    for (key, value) in parsed.policies {
        match ToolPolicy::parse(&value) {
            Some(p) => policies.insert(&key, p),
            None => tracing::warn!(key, value, "mcp toolPolicies 非法值（allow|ask|deny），跳过"),
        }
    }
    (servers, policies)
}

/// 双 scope 合并：项目覆盖用户同名 server 与同键 policy。项目部分只在已信任时读。
pub fn load(workdir: &Path, project_trusted: bool) -> (Vec<ServerConfig>, PolicySet) {
    let mut out: HashMap<String, ServerConfig> = HashMap::new();
    let mut policies = PolicySet::default();
    let (cfgs, ps) = load_file(&crate::core::paths::config_dir().join("mcp.json"));
    for cfg in cfgs {
        out.insert(cfg.name().to_string(), cfg);
    }
    policies.extend(ps);
    if project_trusted {
        let (cfgs, ps) = load_file(&workdir.join(".mcp.json"));
        for cfg in cfgs {
            out.insert(cfg.name().to_string(), cfg);
        }
        policies.extend(ps);
    }
    (out.into_values().collect(), policies)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, text: &str) -> std::path::PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(".mcp.json");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn parses_stdio_and_remote_and_policies() {
        let dir = std::env::temp_dir().join(format!("kxen-mcp-cfg-{}", std::process::id()));
        let path = write(
            &dir,
            r#"{
            "mcpServers": {
                "fs": {"command": "npx", "args": ["-y", "srv"], "type": "stdio"},
                "web": {"type": "http", "url": "https://x.example/mcp", "headers": {"Authorization": "Bearer t"}},
                "old": {"url": "https://y.example/sse", "transport": "sse"},
                "bad": {"url": "ftp://z.example/x"}
            },
            "toolPolicies": {"fs": "ask", "fs.read_file": "allow", "web": "deny", "oops": "maybe"}
        }"#,
        );
        let (cfgs, policies) = load_file(&path);
        assert_eq!(cfgs.len(), 3, "非法 scheme 必须跳过: {cfgs:?}");
        let web = cfgs.iter().find(|c| c.name() == "web").unwrap();
        assert_eq!(web.transport_kind(), "http");
        assert_eq!(web.url(), Some("https://x.example/mcp"));
        let old = cfgs.iter().find(|c| c.name() == "old").unwrap();
        assert_eq!(old.transport_kind(), "sse", "transport 键与 type 键都收");
        assert_eq!(policies.for_tool("fs", "read_file"), ToolPolicy::Allow);
        assert_eq!(policies.for_tool("fs", "write_file"), ToolPolicy::Ask);
        assert_eq!(policies.for_tool("web", "anything"), ToolPolicy::Deny);
        assert_eq!(policies.for_tool("unknown", "x"), ToolPolicy::Allow, "缺省 Allow（WHY 见 for_tool）");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parses_remote_oauth_object() {
        let dir = std::env::temp_dir().join(format!("kxen-mcp-oauth-cfg-{}", std::process::id()));
        let path = write(
            &dir,
            r#"{"mcpServers": {
            "full": {"url": "https://x.example/mcp", "oauth": {
                "clientId": "cid", "clientSecret": "sec", "callbackPort": 19876,
                "scopes": "mcp read", "authServerMetadataUrl": "https://as.example/meta"
            }},
            "bare": {"url": "https://y.example/mcp"}
        }}"#,
        );
        let (cfgs, _) = load_file(&path);
        assert_eq!(cfgs.len(), 2);
        let full = cfgs.iter().find(|c| c.name() == "full").unwrap();
        let ServerConfig::Remote(rc) = full else { panic!("full 必须是 remote") };
        let oauth = rc.oauth.as_ref().expect("oauth 对象必须解析");
        assert_eq!(oauth.client_id.as_deref(), Some("cid"));
        assert_eq!(oauth.client_secret.as_deref(), Some("sec"));
        assert_eq!(oauth.callback_port, Some(19876));
        assert_eq!(oauth.scopes.as_deref(), Some("mcp read"));
        assert_eq!(oauth.auth_server_metadata_url.as_deref(), Some("https://as.example/meta"));
        let bare = cfgs.iter().find(|c| c.name() == "bare").unwrap();
        let ServerConfig::Remote(rc) = bare else { panic!("bare 必须是 remote") };
        assert!(rc.oauth.is_none(), "无 oauth 键必须为 None");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn infers_kind_from_command_or_url() {
        let dir = std::env::temp_dir().join(format!("kxen-mcp-infer-{}", std::process::id()));
        let path = write(
            &dir,
            r#"{"mcpServers": {
            "a": {"command": "srv"},
            "b": {"url": "https://b.example/mcp"}
        }}"#,
        );
        let (cfgs, _) = load_file(&path);
        assert_eq!(cfgs.len(), 2);
        let a = cfgs.iter().find(|c| c.name() == "a").unwrap();
        assert_eq!(a.transport_kind(), "stdio");
        let b = cfgs.iter().find(|c| c.name() == "b").unwrap();
        assert_eq!(b.transport_kind(), "http", "url 缺省推断 streamable http");
        std::fs::remove_dir_all(&dir).ok();
    }
}
