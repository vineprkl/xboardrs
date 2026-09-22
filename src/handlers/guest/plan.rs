use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};

use crate::common::{ApiResponse, AppError, AppState};

/// GET /api/v1/guest/plan/fetch
pub async fn fetch(State(state): State<AppState>) -> Result<Response, AppError> {
    let plans = state.plan_service.get_available_plans().await?;
    Ok(Json(ApiResponse::success(plans)).into_response())
}
