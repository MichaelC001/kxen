use super::*;

impl McpManager {
    pub fn new() -> Arc<Self> {
        Self::new_inner(None, None, None, None)
    }

    pub fn new_with_execution_approval(
        broker: Arc<crate::agent::approval::ApprovalBroker>,
        bus: crate::core::event::EventBus,
    ) -> Arc<Self> {
        Self::new_inner(Some((broker, bus)), None, None, None)
    }

    pub fn new_with_execution_auto(
        auto: Arc<dyn crate::tools::auto_approve::AutoApprove>,
        remote_mcp_enabled: bool,
        stdio_environment: crate::agent::agent_loop::ChildEnvironment,
    ) -> Arc<Self> {
        Self::new_inner(None, Some(auto), Some(remote_mcp_enabled), Some(stdio_environment))
    }

    fn new_inner(
        execution_approval: Option<(Arc<crate::agent::approval::ApprovalBroker>, crate::core::event::EventBus)>,
        execution_auto: Option<Arc<dyn crate::tools::auto_approve::AutoApprove>>,
        remote_mcp_override: Option<bool>,
        stdio_environment: Option<crate::agent::agent_loop::ChildEnvironment>,
    ) -> Arc<Self> {
        Arc::new(Self {
            servers: Mutex::new(HashMap::new()),
            policies: Mutex::new(PolicySet::default()),
            roots: Mutex::new(Arc::new(Vec::new())),
            reload_lock: tokio::sync::Mutex::new(()),
            lifecycle: Mutex::new(HashMap::new()),
            next_generation: std::sync::atomic::AtomicU64::new(1),
            execution_approval,
            execution_auto,
            stdio_environment,
            remote_mcp_override,
            approved_project_stdio: Mutex::new(HashSet::new()),
        })
    }
}
