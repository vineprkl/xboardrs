use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{AppError, AppState},
    entities::{notice, Notice},
    handlers::AuthenticatedUser,
};

#[derive(Debug, Deserialize, Default)]
pub struct NoticeQuery {
    pub current: Option<u64>,
}

/// GET /api/v1/user/notice/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Query(query): Query<NoticeQuery>,
) -> Result<Response, AppError> {
    let current = query.current.unwrap_or(1).max(1);
    let page_size = 5u64;

    let query_builder = Notice::find()
        .filter(notice::Column::Show.eq(true))
        .order_by_asc(notice::Column::Sort)
        .order_by_desc(notice::Column::Id);

    let paginator = query_builder.paginate(&state.db, page_size);
    let total = paginator.num_items().await?;
    let items = paginator.fetch_page(current - 1).await?;

    let resp = json!({
        "data": items,
        "total": total,
    });

    Ok(Json(resp).into_response())
}
