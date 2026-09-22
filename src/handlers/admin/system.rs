use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{EntityTrait, PaginatorTrait, QueryOrder, QuerySelect};
use serde_json::json;

use crate::{
    common::{AdminTableQuery, ApiResponse, AppError, AppState, PaginatedResponse},
    entities::{admin_audit_log, AdminAuditLog},
    handlers::auth::AuthenticatedAdmin,
};

/// GET /api/v2/admin/system/getSystemStatus
pub async fn get_system_status(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();
    let data = json!({
        "schedule": true,
        "horizon": true,
        "schedule_last_runtime": now - 30,
        "rust_version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    });
    Ok(ApiResponse::success(data).into_response())
}

/// GET /api/v2/admin/system/getQueueStats
pub async fn get_queue_stats(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    let data = json!({
        "failedJobs": 0,
        "jobsPerMinute": 0,
        "pausedMasters": 0,
        "periods": {
            "failedJobs": 100,
            "recentJobs": 60
        }
    });
    Ok(ApiResponse::success(data).into_response())
}

/// GET /api/v2/admin/system/getQueueWorkload
pub async fn get_queue_workload(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!([])).into_response())
}

/// GET or POST /api/v2/admin/system/getAuditLog
pub async fn get_audit_log(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<AdminTableQuery>,
    body: Option<Json<AdminTableQuery>>,
) -> Result<Response, AppError> {
    let q = body.map(|b| b.0).unwrap_or(query);

    let page = q.page();
    let per_page = q.per_page();
    let offset = q.offset();

    let total = AdminAuditLog::find().count(&state.db).await?;
    let logs = AdminAuditLog::find()
        .order_by_desc(admin_audit_log::Column::CreatedAt)
        .offset(offset)
        .limit(per_page)
        .all(&state.db)
        .await?;

    Ok(PaginatedResponse::new(logs, total, page, per_page).into_response())
}

/// GET /api/v2/admin/system/getQueueMasters
pub async fn get_queue_masters(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!([])).into_response())
}

/// GET /api/v2/admin/system/getHorizonFailedJobs
pub async fn get_horizon_failed_jobs(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(axum::Json(json!({
        "data": [],
        "total": 0
    }))
    .into_response())
}
