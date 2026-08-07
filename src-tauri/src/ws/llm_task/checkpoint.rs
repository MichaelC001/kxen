pub(super) async fn before_run(workdir: &std::path::Path, message_id: &str) -> Result<(), kxen_gui::agent::agent_loop::AgentEvent> {
    kxen_gui::tools::checkpoint::checkpoint_barrier(workdir, message_id)
        .await
        .map_err(|error| kxen_gui::agent::agent_loop::AgentEvent::Error { message: format!("checkpoint save failed before run: {error}") })
}
