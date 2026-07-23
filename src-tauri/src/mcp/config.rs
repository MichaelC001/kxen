//! .mcp.json 解析：双 scope（项目 <workdir>/.mcp.json 覆盖用户 ~/.config/kxen/mcp.json）。
//! v1 只支持 stdio 类型；http/sse 配置解析保留但标记 unsupported。

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct McpFile {
    #[serde(rename = "mcpServers", default)]
    servers: HashMap<String, ServerDef>,
}

#[derive(Debug, Deserialize)]
struct ServerDef {
    #[serde(rename = "type", default = "stdio_type")]
    kind: String,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    url: Option<String>,
}

fn stdio_type() -> String {
    "stdio".into()
}

fn load_file(path: &Path) -> Vec<ServerConfig> {
    let Ok(text) = std::fs::read_to_string(path) else { return vec![] };
    let parsed: McpFile = match serde_json::from_str(&text) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "mcp.json parse failed");
            return vec![];
        }
    };
    parsed
        .servers
        .into_iter()
        .filter_map(|(name, def)| {
            if def.kind != "stdio" {
                tracing::warn!(name, kind = def.kind, "mcp server type unsupported in v1, skipped");
                return None;
            }
            let command = def.command.or(def.url)?;
            Some(ServerConfig { name, command, args: def.args, env: def.env })
        })
        .collect()
}

/// 双 scope 合并：项目覆盖用户同名 server。项目部分只在已信任时由调用方传入。
pub fn load(workdir: &Path, project_trusted: bool) -> Vec<ServerConfig> {
    let mut out: HashMap<String, ServerConfig> = HashMap::new();
    for cfg in load_file(&crate::core::paths::config_dir().join("mcp.json")) {
        out.insert(cfg.name.clone(), cfg);
    }
    if project_trusted {
        for cfg in load_file(&workdir.join(".mcp.json")) {
            out.insert(cfg.name.clone(), cfg);
        }
    }
    out.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_project_overrides() {
        let dir = std::env::temp_dir().join(format!("kxen-mcp-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers": {"fs": {"command": "npx", "args": ["-y", "srv"], "type": "stdio"}, "remote": {"type": "http", "url": "https://x"}}}"#,
        )
        .unwrap();
        let cfgs = load_file(&dir.join(".mcp.json"));
        assert_eq!(cfgs.len(), 1, "http 类型 v1 跳过");
        assert_eq!(cfgs[0].name, "fs");
        assert_eq!(cfgs[0].args, vec!["-y", "srv"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
