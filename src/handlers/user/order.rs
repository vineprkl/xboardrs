use crate::{
    common::{ApiResponse, AppError, AppState, FormOrJson as Json},
    entities::{order, Order, Payment},
    handlers::auth::AuthenticatedUser,
    services::order_service::{OrderService, STATUS_PENDING},
    services::PaymentService,
};
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct OrderFetchQuery {
    pub status: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct OrderDetailQuery {
    pub trade_no: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OrderSaveRequest {
    pub plan_id: i32,
    pub period: String,
    pub coupon_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OrderCheckoutRequest {
    pub trade_no: String,
    pub method: Option<i32>,
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OrderCancelRequest {
    pub trade_no: String,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    #[serde(rename = "type")]
    pub pay_type: i8,
    pub data: Value,
}

/// GET /api/v1/user/order/fetch
pub async fn fetch(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(query): Query<OrderFetchQuery>,
) -> Result<Response, AppError> {
    let order_service = OrderService::new(
        state.db.clone(),
        state.plan_service.clone(),
        crate::services::CouponService::new(state.db.clone()),
        state.setting_service.clone(),
    );

    let mut select = Order::find().filter(order::Column::UserId.eq(auth_user.0.id));
    if let Some(status) = query.status {
        select = select.filter(order::Column::Status.eq(status));
    }
    let orders = select
        .order_by_desc(order::Column::CreatedAt)
        .all(&state.db)
        .await?;

    let mut list = Vec::new();
    for o in orders {
        list.push(order_service.format_order(&o, false).await?);
    }

    Ok(ApiResponse::success(list).into_response())
}

/// GET /api/v1/user/order/detail
pub async fn detail(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(query): Query<OrderDetailQuery>,
) -> Result<Response, AppError> {
    let trade_no = query
        .trade_no
        .ok_or_else(|| AppError::Custom(422, "trade_no is required".to_string()))?;

    let order = Order::find()
        .filter(order::Column::UserId.eq(auth_user.0.id))
        .filter(order::Column::TradeNo.eq(&trade_no))
        .one(&state.db)
        .await?
        .ok_or_else(|| {
            AppError::Custom(400, "Order does not exist or has been paid".to_string())
        })?;

    let order_service = OrderService::new(
        state.db.clone(),
        state.plan_service.clone(),
        crate::services::CouponService::new(state.db.clone()),
        state.setting_service.clone(),
    );

    let dto = order_service.format_order(&order, true).await?;
    Ok(ApiResponse::success(dto).into_response())
}

/// POST /api/v1/user/order/save
pub async fn save(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<OrderSaveRequest>,
) -> Result<Response, AppError> {
    let order_service = OrderService::new(
        state.db.clone(),
        state.plan_service.clone(),
        crate::services::CouponService::new(state.db.clone()),
        state.setting_service.clone(),
    );

    let order = order_service
        .create_from_request(
            auth_user.0.id,
            payload.plan_id,
            &payload.period,
            payload.coupon_code.as_deref(),
        )
        .await?;

    Ok(ApiResponse::success(order.trade_no).into_response())
}

/// POST /api/v1/user/order/checkout
pub async fn checkout(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<OrderCheckoutRequest>,
) -> Result<Response, AppError> {
    let order = Order::find()
        .filter(order::Column::TradeNo.eq(&payload.trade_no))
        .filter(order::Column::UserId.eq(auth_user.0.id))
        .filter(order::Column::Status.eq(STATUS_PENDING))
        .one(&state.db)
        .await?
        .ok_or_else(|| {
            AppError::Custom(400, "Order does not exist or has been paid".to_string())
        })?;

    if order.total_amount < 0 {
        return Err(AppError::Custom(400, "Order amount is invalid".to_string()));
    }

    let order_service = OrderService::new(
        state.db.clone(),
        state.plan_service.clone(),
        crate::services::CouponService::new(state.db.clone()),
        state.setting_service.clone(),
    );

    // Free order or fully covered by account balance
    if order.total_amount <= 0 {
        order_service.paid(&order.trade_no, &order.trade_no).await?;
        return Ok(Json(json!({
            "type": -1,
            "data": true
        }))
        .into_response());
    }

    let method_id = payload
        .method
        .ok_or_else(|| AppError::Custom(400, "Payment method is not available".to_string()))?;

    let payment_model = Payment::find_by_id(method_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400, "Payment method is not available".to_string()))?;

    if !payment_model.enable {
        return Err(AppError::Custom(
            400,
            "Payment method is not available".to_string(),
        ));
    }

    // Calculate handling fees if configured
    let mut handling_amount = 0;
    if let Some(percent) = payment_model.handling_fee_percent {
        if percent > 0.0 {
            handling_amount += ((order.total_amount as f64) * (percent / 100.0)).round() as i32;
        }
    }
    if let Some(fixed) = payment_model.handling_fee_fixed {
        if fixed > 0 {
            handling_amount += fixed;
        }
    }

    let mut o_active: order::ActiveModel = order.clone().into();
    o_active.payment_id = Set(Some(method_id));
    if handling_amount > 0 {
        o_active.handling_amount = Set(Some(handling_amount));
    }
    o_active.update(&state.db).await?;

    let final_amount = order.total_amount + handling_amount;

    let payment_service = PaymentService::new(state.db.clone(), state.setting_service.clone());
    let pay_result = payment_service
        .pay(
            method_id,
            &order.trade_no,
            final_amount,
            auth_user.0.id,
            payload.token,
        )
        .await?;

    Ok(Json(json!({
        "type": pay_result.pay_type,
        "data": pay_result.data
    }))
    .into_response())
}

/// GET /api/v1/user/order/check
pub async fn check(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(query): Query<OrderDetailQuery>,
) -> Result<Response, AppError> {
    let trade_no = query
        .trade_no
        .ok_or_else(|| AppError::Custom(422, "trade_no is required".to_string()))?;

    let order = Order::find()
        .filter(order::Column::UserId.eq(auth_user.0.id))
        .filter(order::Column::TradeNo.eq(&trade_no))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400, "Order does not exist".to_string()))?;

    Ok(ApiResponse::success(order.status).into_response())
}

/// GET /api/v1/user/order/getPaymentMethod
pub async fn get_payment_method(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<Response, AppError> {
    let payment_service = PaymentService::new(state.db.clone(), state.setting_service.clone());
    let methods = payment_service.get_payment_methods().await?;
    Ok(ApiResponse::success(methods).into_response())
}

/// POST /api/v1/user/order/cancel
pub async fn cancel(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<OrderCancelRequest>,
) -> Result<Response, AppError> {
    let order_service = OrderService::new(
        state.db.clone(),
        state.plan_service.clone(),
        crate::services::CouponService::new(state.db.clone()),
        state.setting_service.clone(),
    );

    order_service
        .cancel(&payload.trade_no, auth_user.0.id)
        .await?;
    Ok(ApiResponse::success(true).into_response())
}
