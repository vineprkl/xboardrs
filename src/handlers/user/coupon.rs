use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    common::{ApiResponse, AppError, AppState, FormOrJson as Json},
    handlers::AuthenticatedUser,
};

#[derive(Debug, Deserialize, Default)]
pub struct CouponCheckRequest {
    pub code: Option<String>,
    pub plan_id: Option<i32>,
    pub period: Option<String>,
}

/// POST /api/v1/user/coupon/check
pub async fn check(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<CouponCheckRequest>,
) -> Result<Response, AppError> {
    let code = match payload.code {
        Some(ref c) if !c.trim().is_empty() => c.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Coupon cannot be empty".into(),
            ))
        }
    };

    let coupon_res = state
        .coupon_service
        .check_coupon(code, user.id, payload.plan_id, payload.period.as_deref())
        .await?;

    Ok(Json(ApiResponse::success(coupon_res)).into_response())
}
