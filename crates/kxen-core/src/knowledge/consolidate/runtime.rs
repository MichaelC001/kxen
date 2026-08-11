static PASS_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) struct PassGuard;

impl Drop for PassGuard {
    fn drop(&mut self) {
        PASS_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    }
}

pub(super) fn try_acquire_pass() -> Option<PassGuard> {
    PASS_ACTIVE.compare_exchange(false, true, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire).ok()?;
    Some(PassGuard)
}

pub struct ConsolidationResult {
    pub written: usize,
    pub diagnostics: Vec<String>,
}

pub struct SessionRoute {
    pub mrm: std::sync::Arc<crate::llm::mrm::ModelResourceManager>,
    pub model: crate::llm::ModelRef,
}

/// 启动回执压缩必须保留仍有 durable Knowledge 回放标记的 operation。
/// 否则 usage commit 与 `metering_ack` commit 之间崩溃，恢复会把同一调用计两次。
pub fn pending_metering_operation_ids() -> Result<std::collections::HashSet<String>, String> {
    let root = super::attempt::root();
    let mut operation_ids = std::collections::HashSet::new();
    for session_id in super::attempt::session_ids(&root)? {
        let Some(current) = super::attempt::load(&root, &session_id)? else { continue };
        crate::core::ids::validate_id(&current.operation_id)?;
        operation_ids.insert(current.operation_id);
    }
    Ok(operation_ids)
}
