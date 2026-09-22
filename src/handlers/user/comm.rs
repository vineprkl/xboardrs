use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use sea_orm::EntityTrait;
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError, AppState, FormOrJson as Json},
    entities::Payment,
    handlers::AuthenticatedUser,
};

#[derive(Debug, Deserialize, Default)]
pub struct StripePkRequest {
    pub id: Option<i32>,
}

const DEFAULT_WITHDRAW_METHODS: &[&str] = &["alipay", "usdt"];

/// GET /api/v1/user/comm/config
pub async fn config(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Response, AppError> {
    let is_telegram = if state
        .setting_service
        .get_bool("telegram_bot_enable", false)
        .await
    {
        1
    } else {
        0
    };
    let telegram_discuss_link = state
        .setting_service
        .get_optional_string("telegram_discuss_link")
        .await;
    let stripe_pk = state
        .setting_service
        .get_optional_string("stripe_pk_live")
        .await;

    let methods_raw = state
        .setting_service
        .get_string("commission_withdraw_method", "")
        .await;
    let withdraw_methods: Vec<String> = if methods_raw.trim().is_empty() {
        DEFAULT_WITHDRAW_METHODS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else if let Ok(arr) = serde_json::from_str::<Vec<String>>(&methods_raw) {
        arr
    } else {
        methods_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };

    let withdraw_close = if state
        .setting_service
        .get_bool("withdraw_close_enable", false)
        .await
    {
        1
    } else {
        0
    };
    let currency = state.setting_service.get_string("currency", "CNY").await;
    let currency_symbol = state
        .setting_service
        .get_string("currency_symbol", "¥")
        .await;
    let comm_dist_enable = if state
        .setting_service
        .get_bool("commission_distribution_enable", false)
        .await
    {
        1
    } else {
        0
    };
    let comm_l1 = state
        .setting_service
        .get_optional_string("commission_distribution_l1")
        .await;
    let comm_l2 = state
        .setting_service
        .get_optional_string("commission_distribution_l2")
        .await;
    let comm_l3 = state
        .setting_service
        .get_optional_string("commission_distribution_l3")
        .await;

    let data = json!({
        "is_telegram": is_telegram,
        "telegram_discuss_link": telegram_discuss_link,
        "stripe_pk": stripe_pk,
        "withdraw_methods": withdraw_methods,
        "withdraw_close": withdraw_close,
        "currency": currency,
        "currency_symbol": currency_symbol,
        "commission_distribution_enable": comm_dist_enable,
        "commission_distribution_l1": comm_l1,
        "commission_distribution_l2": comm_l2,
        "commission_distribution_l3": comm_l3,
    });

    Ok(Json(ApiResponse::success(data)).into_response())
}

/// POST /api/v1/user/comm/getStripePublicKey
pub async fn get_stripe_public_key(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(payload): Json<StripePkRequest>,
) -> Result<Response, AppError> {
    let payment_id = payload
        .id
        .ok_or_else(|| AppError::UnprocessableEntity("payment id is required".into()))?;

    let pay = Payment::find_by_id(payment_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::BadRequest("payment is not found".into()))?;

    if pay.payment != "StripeCredit" {
        return Err(AppError::BadRequest("payment is not found".into()));
    }

    let config_json: serde_json::Value = serde_json::from_str(&pay.config).unwrap_or(json!({}));

    let pk = config_json
        .get("stripe_pk_live")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    Ok(Json(ApiResponse::success(pk)).into_response())
}
