use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError, AppState, FormOrJson as Json},
    handlers::AuthenticatedUser,
    services::GiftCardService,
};

#[derive(Debug, Deserialize, Default)]
pub struct GiftCardCodeRequest {
    pub code: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct GiftCardHistoryQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct GiftCardDetailQuery {
    pub id: Option<i32>,
}

/// POST /api/v1/user/gift-card/check
pub async fn check(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<GiftCardCodeRequest>,
) -> Result<Response, AppError> {
    let code = match payload.code {
        Some(ref c) if !c.trim().is_empty() => c.trim(),
        _ => return Err(AppError::UnprocessableEntity("兑换码不能为空".into())),
    };

    let res = state.gift_card_service.check_code(code, &user).await?;
    Ok(Json(ApiResponse::success(res)).into_response())
}

/// POST /api/v1/user/gift-card/redeem
pub async fn redeem(
    State(state): State<AppState>,
    headers: HeaderMap,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<GiftCardCodeRequest>,
) -> Result<Response, AppError> {
    let code = match payload.code {
        Some(ref c) if !c.trim().is_empty() => c.trim(),
        _ => return Err(AppError::UnprocessableEntity("兑换码不能为空".into())),
    };

    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|h| h.to_str().ok());

    let res = state
        .gift_card_service
        .redeem_code(code, &user, user_agent)
        .await?;

    Ok(Json(ApiResponse::success(res)).into_response())
}

/// GET /api/v1/user/gift-card/history
pub async fn history(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(query): Query<GiftCardHistoryQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(15).clamp(1, 100);

    let (data, total, last_page) = state
        .gift_card_service
        .fetch_history(user.id, page, per_page)
        .await?;

    let resp = json!({
        "data": data,
        "pagination": {
            "current_page": page,
            "last_page": last_page,
            "per_page": per_page,
            "total": total,
        }
    });

    Ok(Json(resp).into_response())
}

/// GET /api/v1/user/gift-card/detail
pub async fn detail(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(query): Query<GiftCardDetailQuery>,
) -> Result<Response, AppError> {
    let usage_id = query
        .id
        .ok_or_else(|| AppError::UnprocessableEntity("id is required".into()))?;

    let detail = state
        .gift_card_service
        .fetch_detail(user.id, usage_id)
        .await?;

    Ok(Json(ApiResponse::success(detail)).into_response())
}

/// GET /api/v1/user/gift-card/types
pub async fn types() -> Result<Response, AppError> {
    let type_map = GiftCardService::get_type_map();
    let resp = json!({
        "types": type_map,
    });
    Ok(Json(ApiResponse::success(resp)).into_response())
}
