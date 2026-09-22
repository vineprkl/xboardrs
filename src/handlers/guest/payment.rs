use crate::{
    common::{AppError, AppState},
    services::{OrderService, PaymentService},
};
use axum::{
    body::to_bytes,
    extract::{Path, Query, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;

/// Handles incoming payment gateway webhooks / asynchronous notifications.
/// Accepts GET or POST on /api/v1/guest/payment/notify/:method/:uuid
pub async fn notify(
    State(state): State<AppState>,
    Path((method, uuid)): Path<(String, String)>,
    Query(query_params): Query<HashMap<String, String>>,
    request: Request,
) -> Result<Response, AppError> {
    let mut all_params = query_params;

    // Also extract body parameters if POST / PUT
    let content_type = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body_bytes = to_bytes(request.into_body(), usize::MAX)
        .await
        .unwrap_or_default();

    if !body_bytes.is_empty() {
        if content_type.contains("application/json") {
            if let Ok(json_map) =
                serde_json::from_slice::<HashMap<String, serde_json::Value>>(&body_bytes)
            {
                for (k, v) in json_map {
                    let s = match v {
                        serde_json::Value::String(str_val) => str_val,
                        other => other.to_string(),
                    };
                    all_params.insert(k, s);
                }
            }
        } else {
            // Treat as x-www-form-urlencoded
            let body_str = String::from_utf8_lossy(&body_bytes);
            for part in body_str.split('&') {
                if let Some((k, v)) = part.split_once('=') {
                    let key = urlencoding::decode(k)
                        .unwrap_or_else(|_| k.into())
                        .to_string();
                    let val = urlencoding::decode(v)
                        .unwrap_or_else(|_| v.into())
                        .to_string();
                    all_params.insert(key, val);
                }
            }
        }
    }

    let payment_service = PaymentService::new(state.db.clone(), state.setting_service.clone());
    let verify = payment_service.notify(&method, &uuid, &all_params).await?;

    let order_service = OrderService::new(
        state.db.clone(),
        state.plan_service.clone(),
        crate::services::CouponService::new(state.db.clone()),
        state.setting_service.clone(),
    );

    let paid_success = order_service
        .paid(&verify.trade_no, &verify.callback_no)
        .await?;

    if !paid_success {
        return Err(AppError::Custom(400, "handle error".to_string()));
    }

    let custom_res = verify
        .custom_result
        .unwrap_or_else(|| "success".to_string());
    Ok((StatusCode::OK, custom_res).into_response())
}
