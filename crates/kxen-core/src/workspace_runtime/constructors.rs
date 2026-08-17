use super::*;

impl WorkspaceRuntimeRegistry {
    pub fn with_config(user_config: PathBuf, config: crate::core::config::Config) -> Self {
        Self {
            runtimes: Arc::new(std::sync::Mutex::new(HashMap::new())),
            runtime_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            mcp_approval: None,
            mcp_auto: None,
            base_mrm: Arc::new(std::sync::RwLock::new(Arc::new(crate::llm::mrm::ModelResourceManager::new(config)))),
            user_config: Arc::from(user_config),
            config_update_gate: Arc::new(ConfigUpdateGate::default()),
        }
    }

    pub fn with_config_and_mcp_auto(
        user_config: PathBuf,
        config: crate::core::config::Config,
        mcp_auto: Arc<dyn crate::tools::auto_approve::AutoApprove>,
        stdio_environment: crate::agent::agent_loop::ChildEnvironment,
    ) -> Self {
        let remote_mcp_enabled = config.experimental.remote_mcp;
        Self {
            runtimes: Arc::new(std::sync::Mutex::new(HashMap::new())),
            runtime_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            mcp_approval: None,
            mcp_auto: Some((mcp_auto, remote_mcp_enabled, stdio_environment)),
            base_mrm: Arc::new(std::sync::RwLock::new(Arc::new(crate::llm::mrm::ModelResourceManager::new(config)))),
            user_config: Arc::from(user_config),
            config_update_gate: Arc::new(ConfigUpdateGate::default()),
        }
    }

    pub fn with_mcp_execution_approval(
        broker: Arc<crate::agent::approval::ApprovalBroker>,
        bus: crate::core::event::EventBus,
        base_mrm: Arc<std::sync::RwLock<Arc<crate::llm::mrm::ModelResourceManager>>>,
    ) -> Self {
        Self {
            runtimes: Arc::new(std::sync::Mutex::new(HashMap::new())),
            runtime_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            mcp_approval: Some((broker, bus)),
            mcp_auto: None,
            base_mrm,
            user_config: Arc::from(crate::core::paths::KxenPaths::user().config_file()),
            config_update_gate: Arc::new(ConfigUpdateGate::default()),
        }
    }

    pub fn with_user_config(user_config: PathBuf) -> Result<Self, String> {
        let config = crate::core::config::Config::load(&user_config, None).map_err(|error| error.to_string())?;
        Ok(Self::with_config(user_config, config))
    }
}
