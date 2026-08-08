pub(super) async fn before_run(workdir: &std::path::Path, message_id: &str) -> Result<(), kxen_core::agent::agent_loop::AgentEvent> {
    kxen_core::tools::checkpoint::checkpoint_barrier(workdir, message_id)
        .await
        .map_err(|error| kxen_core::agent::agent_loop::AgentEvent::Error { message: format!("checkpoint save failed before run: {error}") })
}
