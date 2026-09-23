use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    common::{AdminTableQuery, ApiResponse, AppError, AppState, PaginatedResponse},
    entities::{coupon, Coupon},
    handlers::auth::AuthenticatedAdmin,
    utils::{parse_bool, parse_i32, parse_i64, random_char},
};

#[derive(Debug, Deserialize)]
pub struct CouponGenerateRequest {
    pub code: Option<String>,
    pub name: String,
    pub r#type: Value,
    pub value: Value,
    pub show: Option<Value>,
    pub limit_use: Option<Value>,
    pub limit_use_with_user: Option<Value>,
    pub limit_plan_ids: Option<Value>,
    pub limit_period: Option<Value>,
    pub started_at: Value,
    pub ended_at: Value,
    pub generate_count: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CouponIdRequest {
    pub id: Value,
    pub show: Option<Value>,
}

/// GET or POST /api/v2/admin/coupon/fetch
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

    let mut select = Coupon::find();

    for f in q.filters() {
        if f.id == "name" {
            if let Some(s) = f.value.as_str() {
                select = select.filter(coupon::Column::Name.contains(s));
            }
        } else if f.id == "code" {
            if let Some(s) = f.value.as_str() {
                select = select.filter(coupon::Column::Code.contains(s));
            }
        }
    }

    let total = select.clone().count(&state.db).await?;
    let coupons = select
        .order_by_desc(coupon::Column::CreatedAt)
        .offset(offset)
        .limit(per_page)
        .all(&state.db)
        .await?;

    let data: Vec<Value> = coupons
        .into_iter()
        .map(|c| {
            let mut val = serde_json::to_value(&c).unwrap_or_default();
            if let Some(obj) = val.as_object_mut() {
                let parse_array = |opt: &Option<String>| -> Value {
                    match opt {
                        Some(s) if !s.trim().is_empty() => {
                            serde_json::from_str::<Value>(s).unwrap_or_else(|_| serde_json::json!([]))
                        }
                        _ => serde_json::json!([]),
                    }
                };
                obj.insert("limit_plan_ids".to_string(), parse_array(&c.limit_plan_ids));
                obj.insert("limit_period".to_string(), parse_array(&c.limit_period));
            }
            val
        })
        .collect();

    Ok(PaginatedResponse::new(data, total, page, per_page).into_response())
}

/// POST /api/v2/admin/coupon/generate
pub async fn generate(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<CouponGenerateRequest>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();
    let count = parse_i32(&payload.generate_count)
        .unwrap_or(1)
        .clamp(1, 1000) as usize;
    let r_type = parse_i32(&Some(payload.r#type)).unwrap_or(1);
    let value = parse_i32(&Some(payload.value)).unwrap_or(0);
    let started_at = parse_i64(&Some(payload.started_at)).unwrap_or(now);
    let ended_at = parse_i64(&Some(payload.ended_at)).unwrap_or(now + 86400 * 30);
    let show = parse_bool(&payload.show).unwrap_or(true);
    let limit_use = parse_i32(&payload.limit_use);
    let limit_use_with_user = parse_i32(&payload.limit_use_with_user);

    let plan_ids_str = payload.limit_plan_ids.map(|v| v.to_string());
    let period_str = payload.limit_period.map(|v| v.to_string());

    let txn = state.db.begin().await?;

    for _ in 0..count {
        let code = payload
            .code
            .clone()
            .unwrap_or_else(|| random_char(8, false).to_uppercase());

        let new_c = coupon::ActiveModel {
            code: Set(code),
            name: Set(payload.name.clone()),
            r#type: Set(r_type),
            value: Set(value),
            show: Set(show),
            limit_use: Set(limit_use),
            limit_use_with_user: Set(limit_use_with_user),
            limit_plan_ids: Set(plan_ids_str.clone()),
            limit_period: Set(period_str.clone()),
            started_at: Set(started_at),
            ended_at: Set(ended_at),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_c.insert(&txn).await?;
    }

    txn.commit().await?;
    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/coupon/drop
pub async fn drop(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<CouponIdRequest>,
) -> Result<Response, AppError> {
    let id = parse_i32(&Some(payload.id))
        .ok_or_else(|| AppError::Custom(400201, "无效的优惠券ID".to_string()))?;
    let c = Coupon::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "优惠券不存在".to_string()))?;

    let c_active: coupon::ActiveModel = c.into();
    c_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/coupon/show
pub async fn show(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<CouponIdRequest>,
) -> Result<Response, AppError> {
    let id = parse_i32(&Some(payload.id))
        .ok_or_else(|| AppError::Custom(400201, "无效的优惠券ID".to_string()))?;
    let c = Coupon::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "优惠券不存在".to_string()))?;

    let mut c_active: coupon::ActiveModel = c.clone().into();
    c_active.show = Set(!c.show);
    c_active.updated_at = Set(chrono::Utc::now().timestamp());
    c_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/coupon/update
pub async fn update(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<CouponIdRequest>,
) -> Result<Response, AppError> {
    let id = parse_i32(&Some(payload.id))
        .ok_or_else(|| AppError::Custom(400201, "无效的优惠券ID".to_string()))?;
    let c = Coupon::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "优惠券不存在".to_string()))?;

    let mut c_active: coupon::ActiveModel = c.into();
    if let Some(show) = parse_bool(&payload.show) {
        c_active.show = Set(show);
    }
    c_active.updated_at = Set(chrono::Utc::now().timestamp());
    c_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}
