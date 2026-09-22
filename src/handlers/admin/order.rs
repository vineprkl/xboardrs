use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{AdminTableQuery, ApiResponse, AppError, AppState, PaginatedResponse},
    entities::{order, user, Order, Plan, User},
    handlers::auth::AuthenticatedAdmin,
    services::order_service::{OrderService, STATUS_CANCELLED, STATUS_PENDING},
    services::PlanService,
};

#[derive(Debug, Deserialize)]
pub struct OrderTradeNoRequest {
    pub trade_no: String,
}

#[derive(Debug, Deserialize)]
pub struct OrderDetailRequest {
    pub id: Option<i32>,
    pub trade_no: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OrderUpdateRequest {
    pub trade_no: String,
    pub commission_status: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct OrderAssignRequest {
    pub plan_id: i32,
    pub email: String,
    pub period: Option<String>,
}

/// GET or POST /api/v2/admin/order/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<AdminTableQuery>,
    body: Option<Json<AdminTableQuery>>,
) -> Result<Response, AppError> {
    let q = body.map(|b| b.0).unwrap_or(query);

    let page = q.page();
    let per_page = q.per_page();
    let offset = q.offset();

    let mut select = Order::find();

    if q.is_commission.unwrap_or(false) {
        select = select
            .filter(order::Column::InviteUserId.is_not_null())
            .filter(order::Column::Status.is_not_in([STATUS_PENDING, STATUS_CANCELLED]))
            .filter(order::Column::CommissionBalance.gt(0));
    }

    // Apply filters
    for f in q.filters() {
        let field = f.id.as_str();
        match field {
            "trade_no" => {
                if let Some(s) = f.value.as_str() {
                    select = select.filter(order::Column::TradeNo.contains(s));
                }
            }
            "user_id" => {
                if let Some(n) = f.value.as_i64() {
                    select = select.filter(order::Column::UserId.eq(n as i32));
                }
            }
            "plan_id" => {
                if let Some(n) = f.value.as_i64() {
                    select = select.filter(order::Column::PlanId.eq(n as i32));
                }
            }
            "status" => {
                if let Some(n) = f.value.as_i64() {
                    select = select.filter(order::Column::Status.eq(n as i32));
                } else if let Some(arr) = f.value.as_array() {
                    let statuses: Vec<i32> = arr
                        .iter()
                        .filter_map(|v| v.as_i64().map(|n| n as i32))
                        .collect();
                    select = select.filter(order::Column::Status.is_in(statuses));
                }
            }
            "commission_status" => {
                if let Some(n) = f.value.as_i64() {
                    select = select.filter(order::Column::CommissionStatus.eq(n as i32));
                }
            }
            _ => {}
        }
    }

    // Sorting
    let sorts = q.sorts();
    if sorts.is_empty() {
        select = select.order_by_desc(order::Column::CreatedAt);
    } else {
        for s in sorts {
            let col = match s.id.as_str() {
                "id" => order::Column::Id,
                "created_at" => order::Column::CreatedAt,
                "total_amount" => order::Column::TotalAmount,
                _ => order::Column::CreatedAt,
            };
            if s.desc {
                select = select.order_by_desc(col);
            } else {
                select = select.order_by_asc(col);
            }
        }
    }

    let total = select.clone().count(&state.db).await?;
    let orders = select.offset(offset).limit(per_page).all(&state.db).await?;

    let mut list = Vec::new();
    for o in orders {
        let plan_obj = Plan::find_by_id(o.plan_id)
            .one(&state.db)
            .await?
            .map(|p| json!({ "id": p.id, "name": p.name }));

        let mut val = serde_json::to_value(&o).unwrap_or_default();
        if let Some(obj) = val.as_object_mut() {
            let legacy_period = PlanService::get_legacy_period(&o.period);
            obj.insert("period".to_string(), json!(legacy_period));
            obj.insert("plan".to_string(), json!(plan_obj));
        }
        list.push(val);
    }

    Ok(PaginatedResponse::new(list, total, page, per_page).into_response())
}

/// POST /api/v2/admin/order/detail
pub async fn detail(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<OrderDetailRequest>,
) -> Result<Response, AppError> {
    let order_opt = if let Some(id) = payload.id {
        Order::find_by_id(id).one(&state.db).await?
    } else if let Some(ref tn) = payload.trade_no {
        Order::find()
            .filter(order::Column::TradeNo.eq(tn))
            .one(&state.db)
            .await?
    } else {
        return Err(AppError::Custom(
            422,
            "id or trade_no is required".to_string(),
        ));
    };

    let o = match order_opt {
        Some(order) => order,
        None => return Err(AppError::Custom(400202, "订单不存在".to_string())),
    };

    let user_obj = User::find_by_id(o.user_id).one(&state.db).await?;
    let plan_obj = Plan::find_by_id(o.plan_id).one(&state.db).await?;
    let inviter_obj = if let Some(iid) = o.invite_user_id {
        User::find_by_id(iid).one(&state.db).await?
    } else {
        None
    };

    let mut surplus_orders = Vec::new();
    if let Some(ref ids_str) = o.surplus_order_ids {
        if let Ok(ids) = serde_json::from_str::<Vec<i32>>(ids_str) {
            surplus_orders = Order::find()
                .filter(order::Column::Id.is_in(ids))
                .all(&state.db)
                .await?;
        }
    }

    let mut val = serde_json::to_value(&o).unwrap_or_default();
    if let Some(obj) = val.as_object_mut() {
        let legacy_period = PlanService::get_legacy_period(&o.period);
        obj.insert("period".to_string(), json!(legacy_period));
        obj.insert("user".to_string(), json!(user_obj));
        obj.insert("plan".to_string(), json!(plan_obj));
        obj.insert("invite_user".to_string(), json!(inviter_obj));
        obj.insert("surplus_orders".to_string(), json!(surplus_orders));
    }

    Ok(ApiResponse::success(val).into_response())
}

/// POST /api/v2/admin/order/paid
pub async fn paid(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<OrderTradeNoRequest>,
) -> Result<Response, AppError> {
    let o = Order::find()
        .filter(order::Column::TradeNo.eq(&payload.trade_no))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "订单不存在".to_string()))?;

    if o.status != STATUS_PENDING {
        return Err(AppError::Custom(
            400,
            "只能对待支付的订单进行操作".to_string(),
        ));
    }

    let order_service = OrderService::new(
        state.db.clone(),
        state.plan_service.clone(),
        crate::services::CouponService::new(state.db.clone()),
        state.setting_service.clone(),
    );

    order_service.paid(&o.trade_no, "manual_operation").await?;
    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/order/cancel
pub async fn cancel(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<OrderTradeNoRequest>,
) -> Result<Response, AppError> {
    let o = Order::find()
        .filter(order::Column::TradeNo.eq(&payload.trade_no))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "订单不存在".to_string()))?;

    if o.status != STATUS_PENDING {
        return Err(AppError::Custom(
            400,
            "只能对待支付的订单进行操作".to_string(),
        ));
    }

    let order_service = OrderService::new(
        state.db.clone(),
        state.plan_service.clone(),
        crate::services::CouponService::new(state.db.clone()),
        state.setting_service.clone(),
    );

    order_service.cancel(&o.trade_no, o.user_id).await?;
    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/order/update
pub async fn update(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<OrderUpdateRequest>,
) -> Result<Response, AppError> {
    let o = Order::find()
        .filter(order::Column::TradeNo.eq(&payload.trade_no))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "订单不存在".to_string()))?;

    let mut o_active: order::ActiveModel = o.into();
    if let Some(cs) = payload.commission_status {
        o_active.commission_status = Set(cs);
    }
    o_active.updated_at = Set(chrono::Utc::now().timestamp());
    o_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/order/assign
pub async fn assign(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<OrderAssignRequest>,
) -> Result<Response, AppError> {
    let plan = Plan::find_by_id(payload.plan_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "该订阅不存在".to_string()))?;

    let user = User::find()
        .filter(user::Column::Email.eq(payload.email.trim()))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "该用户不存在".to_string()))?;

    let period = payload.period.unwrap_or_else(|| "month_price".to_string());
    let period_key = PlanService::get_period_key(&period);

    let order_service = OrderService::new(
        state.db.clone(),
        state.plan_service.clone(),
        crate::services::CouponService::new(state.db.clone()),
        state.setting_service.clone(),
    );

    let order = order_service
        .create_from_request(user.id, plan.id, &period_key, None)
        .await?;

    // Mark paid immediately
    order_service
        .paid(&order.trade_no, "admin_assigned")
        .await?;

    Ok(ApiResponse::success(true).into_response())
}
