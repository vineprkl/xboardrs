use crate::common::{AppError, AppState};

/// Checks node status and cleans up stale online device sessions.
/// Returns the number of stale device sessions pruned.
pub async fn check_and_cleanup_nodes(state: &AppState) -> Result<usize, AppError> {
    let pruned = state.device_state_service.cleanup_stale().await;
    Ok(pruned)
}
