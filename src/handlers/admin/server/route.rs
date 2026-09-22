use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, Set};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{server_route, ServerRoute},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct RouteSaveRequest {
    pub id: Option<i32>,
    pub remarks: String,
    pub r#match: Value,
    pub action: String,
    pub action_value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RouteDropRequest {
    pub id: i32,
}

/// GET /api/v2/admin/server/route/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let routes = ServerRoute::find()
        .order_by_asc(server_route::Column::Id)
        .all(&state.db)
        .await?;

    let mut list = Vec::new();
    for r in routes {
        let match_val: Value =
            serde_json::from_str(&r.r#match).unwrap_or_else(|_| Value::Array(vec![]));
        list.push(serde_json::json!({
            "id": r.id,
            "remarks": r.remarks,
            "match": match_val,
            "action": r.action,
            "action_value": r.action_value,
            "created_at": r.created_at,
            "updated_at": r.updated_at,
        }));
    }

    Ok(ApiResponse::success(list).into_response())
}

/// POST /api/v2/admin/server/route/save
pub async fn save(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<RouteSaveRequest>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();
    let match_str = payload.r#match.to_string();

    if let Some(id) = payload.id {
        let r = ServerRoute::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "路由不存在".to_string()))?;

        let mut r_active: server_route::ActiveModel = r.into();
        r_active.remarks = Set(payload.remarks);
        r_active.r#match = Set(match_str);
        r_active.action = Set(payload.action);
        r_active.action_value = Set(payload.action_value);
        r_active.updated_at = Set(now);
        r_active.update(&state.db).await?;
    } else {
        let new_r = server_route::ActiveModel {
            remarks: Set(payload.remarks),
            r#match: Set(match_str),
            action: Set(payload.action),
            action_value: Set(payload.action_value),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_r.insert(&state.db).await?;
    }

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/route/drop
pub async fn drop(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<RouteDropRequest>,
) -> Result<Response, AppError> {
    let r = ServerRoute::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "路由不存在".to_string()))?;

    let r_active: server_route::ActiveModel = r.into();
    r_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}
