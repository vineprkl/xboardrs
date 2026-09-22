use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError, AppState},
    handlers::AuthenticatedUser,
};

/// GET /api/v1/user/stat/getTrafficLog
pub async fn get_traffic_log(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Response, AppError> {
    let logs = state
        .user_service
        .get_monthly_traffic_logs(&state.db, user.id)
        .await?;

    let data: Vec<serde_json::Value> = logs
        .into_iter()
        .map(|r| {
            json!({
                "u": r.u,
                "d": r.d,
                "record_at": r.record_at,
            })
        })
        .collect();

    Ok(Json(ApiResponse::success(data)).into_response())
}
