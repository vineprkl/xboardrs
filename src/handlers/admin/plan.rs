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
    utils::{parse_bool, parse_i32, parse_i64},
};

#[derive(Debug, Deserialize)]
pub struct PlanSaveRequest {
    pub id: Option<Value>,
    pub group_id: Option<Value>,
    pub transfer_enable: Option<Value>,
    pub name: Option<String>,
    pub speed_limit: Option<Value>,
    pub show: Option<Value>,
    pub sort: Option<Value>,
    pub renew: Option<Value>,
    pub sell: Option<Value>,
    pub prices: Option<Value>,
    pub content: Option<String>,
    pub month_price: Option<Value>,
    pub quarter_price: Option<Value>,
    pub half_year_price: Option<Value>,
    pub year_price: Option<Value>,
    pub two_year_price: Option<Value>,
    pub three_year_price: Option<Value>,
    pub onetime_price: Option<Value>,
    pub reset_price: Option<Value>,
    pub reset_traffic_method: Option<Value>,
    pub capacity_limit: Option<Value>,
    pub device_limit: Option<Value>,
    pub tags: Option<Value>,
    pub force_update: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct PlanDropRequest {
    pub id: Value,
}

#[derive(Debug, Deserialize)]
pub struct PlanUpdateRequest {
    pub id: Value,
    pub show: Option<Value>,
    pub renew: Option<Value>,
    pub sell: Option<Value>,
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

    let stringify = |v: &Option<Value>| -> Option<String> {
        v.as_ref().and_then(|val| {
            if val.is_null() {
                None
            } else if let Value::String(s) = val {
                Some(s.clone())
            } else {
                Some(val.to_string())
            }
        })
    };

    let parsed_id = parse_i32(&payload.id);
    let parsed_group_id = parse_i32(&payload.group_id);
    let parsed_transfer_enable = parse_i64(&payload.transfer_enable);
    let parsed_speed_limit = parse_i32(&payload.speed_limit);
    let parsed_show = parse_bool(&payload.show);
    let parsed_sort = parse_i32(&payload.sort);
    let parsed_renew = parse_bool(&payload.renew);
    let parsed_sell = parse_bool(&payload.sell);
    let parsed_month_price = parse_i32(&payload.month_price);
    let parsed_quarter_price = parse_i32(&payload.quarter_price);
    let parsed_half_year_price = parse_i32(&payload.half_year_price);
    let parsed_year_price = parse_i32(&payload.year_price);
    let parsed_two_year_price = parse_i32(&payload.two_year_price);
    let parsed_three_year_price = parse_i32(&payload.three_year_price);
    let parsed_onetime_price = parse_i32(&payload.onetime_price);
    let parsed_reset_price = parse_i32(&payload.reset_price);
    let parsed_reset_traffic_method = parse_i32(&payload.reset_traffic_method);
    let parsed_capacity_limit = parse_i32(&payload.capacity_limit);
    let parsed_device_limit = parse_i32(&payload.device_limit);
    let parsed_force_update = parse_bool(&payload.force_update);

    if let Some(id) = parsed_id {
        let existing = Plan::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "该订阅不存在".to_string()))?;

        let txn = state.db.begin().await?;

        if parsed_force_update.unwrap_or(false) {
            let gid = parsed_group_id.unwrap_or(existing.group_id);
            let te = parsed_transfer_enable.unwrap_or(existing.transfer_enable);
            let speed = parsed_speed_limit.or(existing.speed_limit);
            let device = parsed_device_limit.or(existing.device_limit);

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
        if let Some(gid) = parsed_group_id {
            p_active.group_id = Set(gid);
        }
        if let Some(te) = parsed_transfer_enable {
            p_active.transfer_enable = Set(te);
        }
        if let Some(name) = payload.name {
            p_active.name = Set(name);
        }
        if payload.speed_limit.is_some() {
            p_active.speed_limit = Set(parsed_speed_limit);
        }
        if let Some(show) = parsed_show {
            p_active.show = Set(show);
        }
        if payload.sort.is_some() {
            p_active.sort = Set(parsed_sort);
        }
        if let Some(renew) = parsed_renew {
            p_active.renew = Set(renew);
        }
        if let Some(sell) = parsed_sell {
            p_active.sell = Set(Some(sell));
        }
        if payload.prices.is_some() {
            p_active.prices = Set(stringify(&payload.prices));
        }
        if payload.content.is_some() {
            p_active.content = Set(payload.content);
        }
        if payload.month_price.is_some() {
            p_active.month_price = Set(parsed_month_price);
        }
        if payload.quarter_price.is_some() {
            p_active.quarter_price = Set(parsed_quarter_price);
        }
        if payload.half_year_price.is_some() {
            p_active.half_year_price = Set(parsed_half_year_price);
        }
        if payload.year_price.is_some() {
            p_active.year_price = Set(parsed_year_price);
        }
        if payload.two_year_price.is_some() {
            p_active.two_year_price = Set(parsed_two_year_price);
        }
        if payload.three_year_price.is_some() {
            p_active.three_year_price = Set(parsed_three_year_price);
        }
        if payload.onetime_price.is_some() {
            p_active.onetime_price = Set(parsed_onetime_price);
        }
        if payload.reset_price.is_some() {
            p_active.reset_price = Set(parsed_reset_price);
        }
        if payload.reset_traffic_method.is_some() {
            p_active.reset_traffic_method = Set(parsed_reset_traffic_method);
        }
        if payload.capacity_limit.is_some() {
            p_active.capacity_limit = Set(parsed_capacity_limit);
        }
        if payload.device_limit.is_some() {
            p_active.device_limit = Set(parsed_device_limit);
        }
        if payload.tags.is_some() {
            p_active.tags = Set(stringify(&payload.tags));
        }
        p_active.updated_at = Set(now);

        p_active.update(&txn).await?;
        txn.commit().await?;

        Ok(ApiResponse::success(true).into_response())
    } else {
        let new_plan = plan::ActiveModel {
            group_id: Set(parsed_group_id.unwrap_or(1)),
            transfer_enable: Set(parsed_transfer_enable.unwrap_or(100)),
            name: Set(payload.name.unwrap_or_else(|| "New Plan".to_string())),
            speed_limit: Set(parsed_speed_limit),
            show: Set(parsed_show.unwrap_or(true)),
            sort: Set(parsed_sort),
            renew: Set(parsed_renew.unwrap_or(true)),
            sell: Set(parsed_sell.or(Some(true))),
            prices: Set(stringify(&payload.prices)),
            content: Set(payload.content),
            month_price: Set(parsed_month_price),
            quarter_price: Set(parsed_quarter_price),
            half_year_price: Set(parsed_half_year_price),
            year_price: Set(parsed_year_price),
            two_year_price: Set(parsed_two_year_price),
            three_year_price: Set(parsed_three_year_price),
            onetime_price: Set(parsed_onetime_price),
            reset_price: Set(parsed_reset_price),
            reset_traffic_method: Set(parsed_reset_traffic_method),
            capacity_limit: Set(parsed_capacity_limit),
            device_limit: Set(parsed_device_limit),
            tags: Set(stringify(&payload.tags)),
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
    let id = parse_i32(&Some(payload.id))
        .ok_or_else(|| AppError::Custom(400201, "无效的订阅ID".to_string()))?;

    let has_orders = Order::find()
        .filter(order::Column::PlanId.eq(id))
        .one(&state.db)
        .await?;
    if has_orders.is_some() {
        return Err(AppError::Custom(
            400201,
            "该订阅下存在订单无法删除".to_string(),
        ));
    }

    let has_users = User::find()
        .filter(user::Column::PlanId.eq(id))
        .one(&state.db)
        .await?;
    if has_users.is_some() {
        return Err(AppError::Custom(
            400201,
            "该订阅下存在用户无法删除".to_string(),
        ));
    }

    let p = Plan::find_by_id(id)
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
    let id = parse_i32(&Some(payload.id))
        .ok_or_else(|| AppError::Custom(400201, "无效的订阅ID".to_string()))?;

    let p = Plan::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "该订阅不存在".to_string()))?;

    let mut p_active: plan::ActiveModel = p.into();
    if let Some(show) = parse_bool(&payload.show) {
        p_active.show = Set(show);
    }
    if let Some(renew) = parse_bool(&payload.renew) {
        p_active.renew = Set(renew);
    }
    if let Some(sell) = parse_bool(&payload.sell) {
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
