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

/// GET /api/v1/user/telegram/getBotInfo
pub async fn get_bot_info(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Response, AppError> {
    let bot_username = state
        .setting_service
        .get_string("telegram_bot_name", "")
        .await;
    let data = json!({
        "username": bot_username,
    });
    Ok(Json(ApiResponse::success(data)).into_response())
}
