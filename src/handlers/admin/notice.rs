use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, Set, TransactionTrait};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{notice, Notice},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct NoticeSaveRequest {
    pub id: Option<i32>,
    pub title: String,
    pub content: String,
    pub img_url: Option<String>,
    pub tags: Option<Value>,
    pub show: Option<bool>,
    pub popup: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct NoticeIdRequest {
    pub id: i32,
}

#[derive(Debug, Deserialize)]
pub struct NoticeSortRequest {
    pub ids: Vec<i32>,
}

/// GET /api/v2/admin/notice/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let notices = Notice::find()
        .order_by_asc(notice::Column::Sort)
        .order_by_desc(notice::Column::Id)
        .all(&state.db)
        .await?;

    Ok(ApiResponse::success(notices).into_response())
}

/// POST /api/v2/admin/notice/save
pub async fn save(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<NoticeSaveRequest>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();
    let tags_str = payload.tags.map(|t| {
        if let Some(s) = t.as_str() {
            s.to_string()
        } else {
            t.to_string()
        }
    });

    if let Some(id) = payload.id {
        let n = Notice::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "公告不存在".to_string()))?;

        let mut n_active: notice::ActiveModel = n.into();
        n_active.title = Set(payload.title);
        n_active.content = Set(payload.content);
        if payload.img_url.is_some() {
            n_active.img_url = Set(payload.img_url);
        }
        if tags_str.is_some() {
            n_active.tags = Set(tags_str);
        }
        if let Some(s) = payload.show {
            n_active.show = Set(s);
        }
        n_active.updated_at = Set(now);
        n_active.update(&state.db).await?;
    } else {
        let new_n = notice::ActiveModel {
            title: Set(payload.title),
            content: Set(payload.content),
            img_url: Set(payload.img_url),
            tags: Set(tags_str),
            show: Set(payload.show.unwrap_or(true)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_n.insert(&state.db).await?;
    }

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/notice/show
pub async fn show(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<NoticeIdRequest>,
) -> Result<Response, AppError> {
    let n = Notice::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "公告不存在".to_string()))?;

    let mut n_active: notice::ActiveModel = n.clone().into();
    n_active.show = Set(!n.show);
    n_active.updated_at = Set(chrono::Utc::now().timestamp());
    n_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/notice/drop
pub async fn drop(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<NoticeIdRequest>,
) -> Result<Response, AppError> {
    let n = Notice::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "公告不存在".to_string()))?;

    let n_active: notice::ActiveModel = n.into();
    n_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/notice/sort
pub async fn sort(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<NoticeSortRequest>,
) -> Result<Response, AppError> {
    let txn = state.db.begin().await?;
    for (idx, id) in payload.ids.iter().enumerate() {
        if let Some(n) = Notice::find_by_id(*id).one(&txn).await? {
            let mut n_active: notice::ActiveModel = n.into();
            n_active.sort = Set(Some((idx + 1) as i32));
            n_active.update(&txn).await?;
        }
    }
    txn.commit().await?;

    Ok(ApiResponse::success(true).into_response())
}
