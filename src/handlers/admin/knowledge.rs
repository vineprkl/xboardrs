use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, Set, TransactionTrait};
use serde::Deserialize;

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{knowledge, Knowledge},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct KnowledgeFetchQuery {
    pub id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct KnowledgeSaveRequest {
    pub id: Option<i32>,
    pub category: String,
    pub title: String,
    pub body: String,
    pub sort: Option<i32>,
    pub show: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct KnowledgeIdRequest {
    pub id: i32,
}

#[derive(Debug, Deserialize)]
pub struct KnowledgeSortRequest {
    pub ids: Vec<i32>,
}

/// GET /api/v2/admin/knowledge/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<KnowledgeFetchQuery>,
) -> Result<Response, AppError> {
    if let Some(id) = query.id {
        let k = Knowledge::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "知识不存在".to_string()))?;
        return Ok(ApiResponse::success(k).into_response());
    }

    let list = Knowledge::find()
        .order_by_asc(knowledge::Column::Sort)
        .all(&state.db)
        .await?;

    Ok(ApiResponse::success(list).into_response())
}

/// GET /api/v2/admin/knowledge/getCategory
pub async fn get_category(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let list = Knowledge::find().all(&state.db).await?;
    let mut cats = Vec::new();
    for k in list {
        if !cats.contains(&k.category) {
            cats.push(k.category);
        }
    }
    Ok(ApiResponse::success(cats).into_response())
}

/// POST /api/v2/admin/knowledge/save
pub async fn save(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<KnowledgeSaveRequest>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();

    if let Some(id) = payload.id {
        let k = Knowledge::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "知识不存在".to_string()))?;

        let mut k_active: knowledge::ActiveModel = k.into();
        k_active.category = Set(payload.category);
        k_active.title = Set(payload.title);
        k_active.body = Set(payload.body);
        if payload.sort.is_some() {
            k_active.sort = Set(payload.sort);
        }
        if let Some(s) = payload.show {
            k_active.show = Set(s);
        }
        k_active.updated_at = Set(now);
        k_active.update(&state.db).await?;
    } else {
        let new_k = knowledge::ActiveModel {
            category: Set(payload.category),
            title: Set(payload.title),
            body: Set(payload.body),
            sort: Set(payload.sort),
            show: Set(payload.show.unwrap_or(true)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_k.insert(&state.db).await?;
    }

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/knowledge/show
pub async fn show(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<KnowledgeIdRequest>,
) -> Result<Response, AppError> {
    let k = Knowledge::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "知识不存在".to_string()))?;

    let mut k_active: knowledge::ActiveModel = k.clone().into();
    k_active.show = Set(!k.show);
    k_active.updated_at = Set(chrono::Utc::now().timestamp());
    k_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/knowledge/drop
pub async fn drop(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<KnowledgeIdRequest>,
) -> Result<Response, AppError> {
    let k = Knowledge::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "知识不存在".to_string()))?;

    let k_active: knowledge::ActiveModel = k.into();
    k_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/knowledge/sort
pub async fn sort(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<KnowledgeSortRequest>,
) -> Result<Response, AppError> {
    let txn = state.db.begin().await?;
    for (idx, id) in payload.ids.iter().enumerate() {
        if let Some(k) = Knowledge::find_by_id(*id).one(&txn).await? {
            let mut k_active: knowledge::ActiveModel = k.into();
            k_active.sort = Set(Some((idx + 1) as i32));
            k_active.update(&txn).await?;
        }
    }
    txn.commit().await?;

    Ok(ApiResponse::success(true).into_response())
}
