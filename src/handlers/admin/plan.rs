use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{order, plan, user, Order, Plan, ServerGroup, User},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct PlanSaveRequest {
    pub id: Option<i32>,
    pub group_id: Option<i32>,
    pub transfer_enable: Option<i64>,
    pub name: Option<String>,
    pub speed_limit: Option<i32>,
    pub show: Option<bool>,
    pub sort: Option<i32>,
    pub renew: Option<bool>,
    pub sell: Option<bool>,
    pub prices: Option<Value>,
    pub content: Option<String>,
    pub month_price: Option<i32>,
    pub quarter_price: Option<i32>,
    pub half_year_price: Option<i32>,
    pub year_price: Option<i32>,
    pub two_year_price: Option<i32>,
    pub three_year_price: Option<i32>,
    pub onetime_price: Option<i32>,
    pub reset_price: Option<i32>,
    pub reset_traffic_method: Option<i32>,
    pub capacity_limit: Option<i32>,
    pub device_limit: Option<i32>,
    pub tags: Option<Value>,
    pub force_update: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PlanDropRequest {
    pub id: i32,
}

#[derive(Debug, Deserialize)]
pub struct PlanUpdateRequest {
    pub id: i32,
    pub show: Option<bool>,
    pub renew: Option<bool>,
    pub sell: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PlanSortRequest {
    pub ids: Vec<i32>,
}

/// GET /api/v2/admin/plan/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let plans = Plan::find()
        .order_by_asc(plan::Column::Sort)
        .all(&state.db)
        .await?;

    let now = chrono::Utc::now().timestamp();
    let mut list = Vec::new();

    for p in plans {
        let group = ServerGroup::find_by_id(p.group_id).one(&state.db).await?;
        let group_val = group.map(|g| json!({ "id": g.id, "name": g.name }));

        let users_count = User::find()
            .filter(user::Column::PlanId.eq(p.id))
            .count(&state.db)
            .await?;

        let active_users_count = User::find()
            .filter(user::Column::PlanId.eq(p.id))
            .filter(
                user::Column::ExpiredAt
                    .is_null()
                    .or(user::Column::ExpiredAt.gt(now)),
            )
            .count(&state.db)
            .await?;

        let mut p_val = serde_json::to_value(&p).unwrap_or_default();
        if let Some(obj) = p_val.as_object_mut() {
            obj.insert("group".to_string(), group_val.unwrap_or(Value::Null));
            obj.insert("users_count".to_string(), json!(users_count));
            obj.insert("active_users_count".to_string(), json!(active_users_count));
        }
        list.push(p_val);
    }

    Ok(ApiResponse::success(list).into_response())
}

/// POST /api/v2/admin/plan/save
pub async fn save(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<PlanSaveRequest>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();

    if let Some(id) = payload.id {
        let existing = Plan::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "该订阅不存在".to_string()))?;

        let txn = state.db.begin().await?;

        if payload.force_update.unwrap_or(false) {
            let gid = payload.group_id.unwrap_or(existing.group_id);
            let te = payload.transfer_enable.unwrap_or(existing.transfer_enable);
            let speed = payload.speed_limit.or(existing.speed_limit);
            let device = payload.device_limit.or(existing.device_limit);

            // Update all users belonging to this plan
            let users = User::find()
                .filter(user::Column::PlanId.eq(id))
                .all(&txn)
                .await?;

            for u in users {
                let mut u_active: user::ActiveModel = u.into();
                u_active.group_id = Set(Some(gid));
                u_active.transfer_enable = Set(te * 1_073_741_824);
                u_active.speed_limit = Set(speed);
                u_active.device_limit = Set(device);
                u_active.updated_at = Set(now);
                u_active.update(&txn).await?;
            }
        }

        let mut p_active: plan::ActiveModel = existing.into();
        if let Some(gid) = payload.group_id {
            p_active.group_id = Set(gid);
        }
        if let Some(te) = payload.transfer_enable {
            p_active.transfer_enable = Set(te);
        }
        if let Some(name) = payload.name {
            p_active.name = Set(name);
        }
        if payload.speed_limit.is_some() {
            p_active.speed_limit = Set(payload.speed_limit);
        }
        if let Some(show) = payload.show {
            p_active.show = Set(show);
        }
        if payload.sort.is_some() {
            p_active.sort = Set(payload.sort);
        }
        if let Some(renew) = payload.renew {
            p_active.renew = Set(renew);
        }
        if payload.sell.is_some() {
            p_active.sell = Set(payload.sell);
        }
        if let Some(ref prices) = payload.prices {
            p_active.prices = Set(Some(prices.to_string()));
        }
        if payload.content.is_some() {
            p_active.content = Set(payload.content);
        }
        if payload.month_price.is_some() {
            p_active.month_price = Set(payload.month_price);
        }
        if payload.quarter_price.is_some() {
            p_active.quarter_price = Set(payload.quarter_price);
        }
        if payload.half_year_price.is_some() {
            p_active.half_year_price = Set(payload.half_year_price);
        }
        if payload.year_price.is_some() {
            p_active.year_price = Set(payload.year_price);
        }
        if payload.two_year_price.is_some() {
            p_active.two_year_price = Set(payload.two_year_price);
        }
        if payload.three_year_price.is_some() {
            p_active.three_year_price = Set(payload.three_year_price);
        }
        if payload.onetime_price.is_some() {
            p_active.onetime_price = Set(payload.onetime_price);
        }
        if payload.reset_price.is_some() {
            p_active.reset_price = Set(payload.reset_price);
        }
        if payload.reset_traffic_method.is_some() {
            p_active.reset_traffic_method = Set(payload.reset_traffic_method);
        }
        if payload.capacity_limit.is_some() {
            p_active.capacity_limit = Set(payload.capacity_limit);
        }
        if payload.device_limit.is_some() {
            p_active.device_limit = Set(payload.device_limit);
        }
        if let Some(ref tags) = payload.tags {
            p_active.tags = Set(Some(tags.to_string()));
        }
        p_active.updated_at = Set(now);

        p_active.update(&txn).await?;
        txn.commit().await?;

        Ok(ApiResponse::success(true).into_response())
    } else {
        let prices_str = payload.prices.map(|v| v.to_string());
        let tags_str = payload.tags.map(|v| v.to_string());

        let new_plan = plan::ActiveModel {
            group_id: Set(payload.group_id.unwrap_or(1)),
            transfer_enable: Set(payload.transfer_enable.unwrap_or(100)),
            name: Set(payload.name.unwrap_or_else(|| "New Plan".to_string())),
            speed_limit: Set(payload.speed_limit),
            show: Set(payload.show.unwrap_or(true)),
            sort: Set(payload.sort),
            renew: Set(payload.renew.unwrap_or(true)),
            sell: Set(payload.sell.or(Some(true))),
            prices: Set(prices_str),
            content: Set(payload.content),
            month_price: Set(payload.month_price),
            quarter_price: Set(payload.quarter_price),
            half_year_price: Set(payload.half_year_price),
            year_price: Set(payload.year_price),
            two_year_price: Set(payload.two_year_price),
            three_year_price: Set(payload.three_year_price),
            onetime_price: Set(payload.onetime_price),
            reset_price: Set(payload.reset_price),
            reset_traffic_method: Set(payload.reset_traffic_method),
            capacity_limit: Set(payload.capacity_limit),
            device_limit: Set(payload.device_limit),
            tags: Set(tags_str),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        new_plan.insert(&state.db).await?;
        Ok(ApiResponse::success(true).into_response())
    }
}

/// POST /api/v2/admin/plan/drop
pub async fn drop(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<PlanDropRequest>,
) -> Result<Response, AppError> {
    let has_orders = Order::find()
        .filter(order::Column::PlanId.eq(payload.id))
        .one(&state.db)
        .await?;
    if has_orders.is_some() {
        return Err(AppError::Custom(
            400201,
            "该订阅下存在订单无法删除".to_string(),
        ));
    }

    let has_users = User::find()
        .filter(user::Column::PlanId.eq(payload.id))
        .one(&state.db)
        .await?;
    if has_users.is_some() {
        return Err(AppError::Custom(
            400201,
            "该订阅下存在用户无法删除".to_string(),
        ));
    }

    let p = Plan::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "该订阅不存在".to_string()))?;

    let p_active: plan::ActiveModel = p.into();
    p_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/plan/update
pub async fn update(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<PlanUpdateRequest>,
) -> Result<Response, AppError> {
    let p = Plan::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "该订阅不存在".to_string()))?;

    let mut p_active: plan::ActiveModel = p.into();
    if let Some(show) = payload.show {
        p_active.show = Set(show);
    }
    if let Some(renew) = payload.renew {
        p_active.renew = Set(renew);
    }
    if let Some(sell) = payload.sell {
        p_active.sell = Set(Some(sell));
    }
    p_active.updated_at = Set(chrono::Utc::now().timestamp());
    p_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/plan/sort
pub async fn sort(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<PlanSortRequest>,
) -> Result<Response, AppError> {
    let txn = state.db.begin().await?;
    for (idx, id) in payload.ids.iter().enumerate() {
        if let Some(p) = Plan::find_by_id(*id).one(&txn).await? {
            let mut p_active: plan::ActiveModel = p.into();
            p_active.sort = Set(Some((idx + 1) as i32));
            p_active.update(&txn).await?;
        }
    }
    txn.commit().await?;

    Ok(ApiResponse::success(true).into_response())
}
