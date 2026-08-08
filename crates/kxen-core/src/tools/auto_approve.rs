//! Shell 自动放行契约（P3 看板级自主授权）：tools 侧只定义接口，实现挂在 kanban 侧，
//! exec 不反向依赖 kanban。
//! `Ok` 的语义是「已放行且审计已 durable」——实现必须先落审计再返回 Ok，绝不「放了但没记」；
//! `Err` 表示不自动放行（未命中/授权失效/审计失败），调用方回落逐次审批路径，原因仅供日志。

pub trait AutoApprove: Send + Sync {
    fn try_auto_allow(&self, command: &str) -> Result<(), String>;
}
