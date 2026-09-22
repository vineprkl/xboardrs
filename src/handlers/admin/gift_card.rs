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
use serde_json::{json, Value};

use crate::{
    common::{AdminTableQuery, ApiResponse, AppError, AppState, PaginatedResponse},
    entities::gift_card::{code, template, usage, GiftCardCode, GiftCardTemplate, GiftCardUsage},
    handlers::auth::AuthenticatedAdmin,
    utils::random_char,
};

#[derive(Debug, Deserialize)]
pub struct TemplateSaveRequest {
    pub id: Option<i32>,
    pub name: String,
    pub description: Option<String>,
    pub r#type: i8,
    pub status: Option<i8>,
    pub rewards: Value,
    pub conditions: Option<Value>,
    pub limits: Option<Value>,
    pub theme_color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TemplateIdRequest {
    pub id: i32,
}

#[derive(Debug, Deserialize)]
pub struct GenerateCodesRequest {
    pub template_id: i32,
    pub count: usize,
    pub expire_days: Option<i64>,
    pub max_usage: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CodeIdRequest {
    pub id: i32,
}

/// GET or POST /api/v2/admin/gift-card/templates
pub async fn templates(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<AdminTableQuery>,
    body: Option<Json<AdminTableQuery>>,
) -> Result<Response, AppError> {
    let q = body.map(|b| b.0).unwrap_or(query);

    let page = q.page();
    let per_page = q.per_page();
    let offset = q.offset();

    let total = GiftCardTemplate::find().count(&state.db).await?;
    let templates = GiftCardTemplate::find()
        .order_by_asc(template::Column::Sort)
        .order_by_desc(template::Column::CreatedAt)
        .offset(offset)
        .limit(per_page)
        .all(&state.db)
        .await?;

    Ok(PaginatedResponse::new(templates, total, page, per_page).into_response())
}

/// POST /api/v2/admin/gift-card/create-template
pub async fn create_template(
    State(state): State<AppState>,
    admin: AuthenticatedAdmin,
    Json(payload): Json<TemplateSaveRequest>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();
    let rewards_str = payload.rewards.to_string();
    let conditions_str = payload.conditions.map(|v| v.to_string());
    let limits_str = payload.limits.map(|v| v.to_string());

    let new_t = template::ActiveModel {
        name: Set(payload.name),
        description: Set(payload.description),
        r#type: Set(payload.r#type),
        status: Set(payload.status.unwrap_or(1)),
        rewards: Set(rewards_str),
        conditions: Set(conditions_str),
        limits: Set(limits_str),
        theme_color: Set(payload.theme_color.unwrap_or_else(|| "#1890ff".to_string())),
        sort: Set(0),
        admin_id: Set(admin.0.id),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    new_t.insert(&state.db).await?;
    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/gift-card/update-template
pub async fn update_template(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<TemplateSaveRequest>,
) -> Result<Response, AppError> {
    let id = payload
        .id
        .ok_or_else(|| AppError::Custom(422, "id is required".to_string()))?;

    let t = GiftCardTemplate::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "模板不存在".to_string()))?;

    let now = chrono::Utc::now().timestamp();
    let mut t_active: template::ActiveModel = t.into();
    t_active.name = Set(payload.name);
    if payload.description.is_some() {
        t_active.description = Set(payload.description);
    }
    t_active.r#type = Set(payload.r#type);
    if let Some(s) = payload.status {
        t_active.status = Set(s);
    }
    t_active.rewards = Set(payload.rewards.to_string());
    if payload.conditions.is_some() {
        t_active.conditions = Set(payload.conditions.map(|v| v.to_string()));
    }
    if payload.limits.is_some() {
        t_active.limits = Set(payload.limits.map(|v| v.to_string()));
    }
    if let Some(tc) = payload.theme_color {
        t_active.theme_color = Set(tc);
    }
    t_active.updated_at = Set(now);
    t_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/gift-card/delete-template
pub async fn delete_template(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<TemplateIdRequest>,
) -> Result<Response, AppError> {
    let t = GiftCardTemplate::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "模板不存在".to_string()))?;

    let t_active: template::ActiveModel = t.into();
    t_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/gift-card/generate-codes
pub async fn generate_codes(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<GenerateCodesRequest>,
) -> Result<Response, AppError> {
    let _t = GiftCardTemplate::find_by_id(payload.template_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "模板不存在".to_string()))?;

    let now = chrono::Utc::now().timestamp();
    let expires_at = payload.expire_days.map(|d| now + d * 86400);
    let max_usage = payload.max_usage.unwrap_or(1);
    let count = payload.count.clamp(1, 5000);
    let batch_id = random_char(12, false);

    let txn = state.db.begin().await?;
    let mut generated = Vec::new();

    for _ in 0..count {
        let code_str = format!("GC-{}", random_char(16, false).to_uppercase());
        let new_c = code::ActiveModel {
            template_id: Set(payload.template_id),
            code: Set(code_str.clone()),
            batch_id: Set(Some(batch_id.clone())),
            status: Set(0),
            usage_count: Set(0),
            max_usage: Set(max_usage),
            expires_at: Set(expires_at),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_c.insert(&txn).await?;
        generated.push(code_str);
    }
    txn.commit().await?;

    Ok(ApiResponse::success(json!({ "batch_id": batch_id, "count": count })).into_response())
}

/// GET or POST /api/v2/admin/gift-card/codes
pub async fn codes(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<AdminTableQuery>,
    body: Option<Json<AdminTableQuery>>,
) -> Result<Response, AppError> {
    let q = body.map(|b| b.0).unwrap_or(query);

    let page = q.page();
    let per_page = q.per_page();
    let offset = q.offset();

    let mut select = GiftCardCode::find();

    for f in q.filters() {
        if f.id == "code" {
            if let Some(s) = f.value.as_str() {
                select = select.filter(code::Column::Code.contains(s));
            }
        } else if f.id == "template_id" {
            if let Some(n) = f.value.as_i64() {
                select = select.filter(code::Column::TemplateId.eq(n as i32));
            }
        } else if f.id == "status" {
            if let Some(n) = f.value.as_i64() {
                select = select.filter(code::Column::Status.eq(n as i8));
            }
        }
    }

    let total = select.clone().count(&state.db).await?;
    let codes = select
        .order_by_desc(code::Column::CreatedAt)
        .offset(offset)
        .limit(per_page)
        .all(&state.db)
        .await?;

    Ok(PaginatedResponse::new(codes, total, page, per_page).into_response())
}

/// POST /api/v2/admin/gift-card/toggle-code
pub async fn toggle_code(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<CodeIdRequest>,
) -> Result<Response, AppError> {
    let c = GiftCardCode::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "卡密不存在".to_string()))?;

    let mut c_active: code::ActiveModel = c.clone().into();
    c_active.status = Set(if c.status == 0 { 2 } else { 0 }); // 0: unused, 2: disabled
    c_active.updated_at = Set(chrono::Utc::now().timestamp());
    c_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/gift-card/delete-code
pub async fn delete_code(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<CodeIdRequest>,
) -> Result<Response, AppError> {
    let c = GiftCardCode::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "卡密不存在".to_string()))?;

    let c_active: code::ActiveModel = c.into();
    c_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// GET /api/v2/admin/gift-card/export-codes
pub async fn export_codes(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let codes = GiftCardCode::find().limit(2000).all(&state.db).await?;
    let mut csv = String::from("id,template_id,code,status,usage_count,max_usage,expires_at\n");
    for c in codes {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            c.id,
            c.template_id,
            c.code,
            c.status,
            c.usage_count,
            c.max_usage,
            c.expires_at.unwrap_or(0)
        ));
    }

    Ok((
        [
            ("content-type", "text/csv; charset=utf-8"),
            (
                "content-disposition",
                "attachment; filename=\"gift_codes.csv\"",
            ),
        ],
        csv,
    )
        .into_response())
}

/// GET or POST /api/v2/admin/gift-card/usages
pub async fn usages(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<AdminTableQuery>,
    body: Option<Json<AdminTableQuery>>,
) -> Result<Response, AppError> {
    let q = body.map(|b| b.0).unwrap_or(query);

    let page = q.page();
    let per_page = q.per_page();
    let offset = q.offset();

    let total = GiftCardUsage::find().count(&state.db).await?;
    let usages = GiftCardUsage::find()
        .order_by_desc(usage::Column::CreatedAt)
        .offset(offset)
        .limit(per_page)
        .all(&state.db)
        .await?;

    Ok(PaginatedResponse::new(usages, total, page, per_page).into_response())
}

/// GET or POST /api/v2/admin/gift-card/statistics
pub async fn statistics(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let total_templates = GiftCardTemplate::find().count(&state.db).await?;
    let total_codes = GiftCardCode::find().count(&state.db).await?;
    let used_codes = GiftCardCode::find()
        .filter(code::Column::Status.eq(1))
        .count(&state.db)
        .await?;

    let res = json!({
        "total_templates": total_templates,
        "total_codes": total_codes,
        "used_codes": used_codes,
        "unused_codes": total_codes.saturating_sub(used_codes),
    });

    Ok(ApiResponse::success(res).into_response())
}

/// GET /api/v2/admin/gift-card/types
pub async fn types(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    let types = json!([
        { "type": 1, "name": "余额充值卡" },
        { "type": 2, "name": "流量叠加包" },
        { "type": 3, "name": "时长延期卡" },
        { "type": 4, "name": "套餐兑换卡" }
    ]);
    Ok(ApiResponse::success(types).into_response())
}
