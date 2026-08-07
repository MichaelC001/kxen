//! [web]/[tray] config section（config.rs 350 行门禁拆分）。

use serde::{Deserialize, Serialize};

/// 内嵌 Web 服务：`/ws` 在桌面端常驻（webview 自用），`enabled` 只管浏览器访问（静态托管）。
/// 桌面端 bind 保持 127.0.0.1，对外暴露由 kxen-web `--bind` 负责。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    /// 浏览器访问（静态托管）开关；tray 可启停并持久化
    pub enabled: bool,
    /// 监听地址（缺省 127.0.0.1）
    pub bind: String,
    /// 优先端口，占用回退随机
    pub port: u16,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self { enabled: true, bind: "127.0.0.1".into(), port: 7824 }
    }
}

impl WebConfig {
    pub(crate) fn validate(&self, source: &str) -> crate::core::Result<()> {
        if self.bind.parse::<std::net::IpAddr>().is_err() {
            return Err(crate::core::Error::Custom(format!("config validate {source}: web.bind must be an IP address")));
        }
        Ok(())
    }
}

/// 系统托盘偏好（存客户端本地 config，不进 server RPC）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrayConfig {
    /// tray 左键默认动作：window（聚焦主窗口）| browser（系统浏览器打开带 token URL）
    pub default_open: String,
    /// 关窗最小化到托盘（false 时关窗即退出）
    pub close_to_tray: bool,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self { default_open: "window".into(), close_to_tray: true }
    }
}

impl TrayConfig {
    pub(crate) fn validate(&self, source: &str) -> crate::core::Result<()> {
        if !matches!(self.default_open.as_str(), "window" | "browser") {
            return Err(crate::core::Error::Custom(format!("config validate {source}: tray.default_open must be window or browser")));
        }
        Ok(())
    }
}
