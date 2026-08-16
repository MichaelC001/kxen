//! [sandbox] config section：QuickJS 沙箱（workflow / 动态工具）资源上限。
//! 仅用户级（不在项目配置白名单内）：沙箱边界不许由仓库内配置放宽。

use serde::{Deserialize, Serialize};

pub const DEFAULT_WORKFLOW_TIMEOUT_SECONDS: u64 = 10 * 60;
pub const DEFAULT_MEMORY_LIMIT_MB: u64 = 64;
pub const DEFAULT_DYNAMIC_TOOL_TIMEOUT_SECONDS: u64 = 5 * 60;
pub const DEFAULT_DYNAMIC_TOOL_MAX_IMPLEMENTATION_CHARS: u32 = 20_000;

/// QuickJS 沙箱上限：None / 0 均取缺省。栈深（1MB）与单次运行 agent 派发数（32）保持内置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SandboxConfig {
    /// workflow 墙钟超时（秒）；缺省 600
    pub workflow_timeout_seconds: Option<u64>,
    /// QuickJS 堆内存上限（MB，workflow 与动态工具宿主共用）；缺省 64
    pub memory_limit_mb: Option<u64>,
    /// 动态工具沙箱超时（秒）；缺省 300
    pub dynamic_tool_timeout_seconds: Option<u64>,
    /// 动态工具实现源码上限（字符；审批卡与会话快照都要承载全文）；缺省 20000
    pub dynamic_tool_max_implementation_chars: Option<u32>,
}

impl SandboxConfig {
    pub fn workflow_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.workflow_timeout_seconds.filter(|v| *v > 0).unwrap_or(DEFAULT_WORKFLOW_TIMEOUT_SECONDS))
    }

    pub fn memory_limit(&self) -> usize {
        (self.memory_limit_mb.filter(|v| *v > 0).unwrap_or(DEFAULT_MEMORY_LIMIT_MB) as usize) * 1024 * 1024
    }

    pub fn dynamic_tool_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.dynamic_tool_timeout_seconds.filter(|v| *v > 0).unwrap_or(DEFAULT_DYNAMIC_TOOL_TIMEOUT_SECONDS))
    }

    pub fn dynamic_tool_max_implementation_chars(&self) -> usize {
        self.dynamic_tool_max_implementation_chars.filter(|v| *v > 0).unwrap_or(DEFAULT_DYNAMIC_TOOL_MAX_IMPLEMENTATION_CHARS) as usize
    }
}

/// 每次调用都查：走 mtime 缓存（同 experimental_config 口径），不逐调用全量读盘解析。
pub fn sandbox_config() -> SandboxConfig {
    crate::core::config_cache::cached_user_config().map(|c| c.sandbox.clone()).unwrap_or_default()
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
