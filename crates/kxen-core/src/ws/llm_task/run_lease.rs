pub(super) async fn acquire(
    sessions_dir: &std::path::Path,
    session_id: &str,
    queue_handoff: bool,
) -> Result<kxen_core::agent::dcp::SessionRunLease, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match kxen_core::agent::dcp::SessionRunLease::try_acquire(sessions_dir, session_id) {
            Ok(lease) => return Ok(lease),
            Err(error) if queue_handoff && std::time::Instant::now() < deadline => {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                tracing::trace!(session = session_id, %error, "waiting for prior queue run lease handoff");
            }
            Err(error) => return Err(error),
        }
    }
}
