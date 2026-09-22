use axum::{
    extract::{Path, Query, State},
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
    entities::{traffic_reset_log, user, TrafficResetLog, User},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct ResetUserRequest {
    pub user_id: i32,
}

/// GET /api/v2/admin/traffic-reset/logs
pub async fn logs(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<AdminTableQuery>,
) -> Result<Response, AppError> {
    let page = query.page();
    let per_page = query.per_page();
    let offset = query.offset();

    let total = TrafficResetLog::find().count(&state.db).await?;
    let logs = TrafficResetLog::find()
        .order_by_desc(traffic_reset_log::Column::ResetTime)
        .offset(offset)
        .limit(per_page)
        .all(&state.db)
        .await?;

    Ok(PaginatedResponse::new(logs, total, page, per_page).into_response())
}

/// GET /api/v2/admin/traffic-reset/stats
pub async fn stats(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let total_resets = TrafficResetLog::find().count(&state.db).await?;
    let res = json!({
        "total_resets": total_resets
    });
    Ok(ApiResponse::success(res).into_response())
}

/// GET /api/v2/admin/traffic-reset/user/:userId/history
pub async fn user_history(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Path(user_id): Path<i32>,
) -> Result<Response, AppError> {
    let logs = TrafficResetLog::find()
        .filter(traffic_reset_log::Column::UserId.eq(user_id))
        .order_by_desc(traffic_reset_log::Column::ResetTime)
        .limit(50)
        .all(&state.db)
        .await?;

    Ok(ApiResponse::success(logs).into_response())
}

/// POST /api/v2/admin/traffic-reset/reset-user
pub async fn reset_user(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<ResetUserRequest>,
) -> Result<Response, AppError> {
    let u = User::find_by_id(payload.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "用户不存在".to_string()))?;

    let now = chrono::Utc::now().timestamp();
    let old_u = u.u;
    let old_d = u.d;

    let mut u_active: user::ActiveModel = u.into();
    u_active.u = Set(0);
    u_active.d = Set(0);
    u_active.updated_at = Set(now);
    u_active.update(&state.db).await?;

    let reset_log = traffic_reset_log::ActiveModel {
        user_id: Set(payload.user_id),
        reset_type: Set("manual".to_string()),
        reset_time: Set(now),
        old_upload: Set(old_u),
        old_download: Set(old_d),
        old_total: Set(old_u + old_d),
        new_upload: Set(0),
        new_download: Set(0),
        new_total: Set(0),
        trigger_source: Set("admin".to_string()),
        metadata: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    reset_log.insert(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}
