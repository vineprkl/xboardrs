use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{plan, server_group, user, Plan, Server, ServerGroup, User},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct GroupSaveRequest {
    pub id: Option<i32>,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct GroupDropRequest {
    pub id: i32,
}

/// GET /api/v2/admin/server/group/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let groups = ServerGroup::find()
        .order_by_desc(server_group::Column::Id)
        .all(&state.db)
        .await?;

    let mut list = Vec::new();
    for g in groups {
        let users_count = User::find()
            .filter(user::Column::GroupId.eq(g.id))
            .count(&state.db)
            .await?;

        // In SeaORM, check servers where group_ids contains g.id
        let all_servers = Server::find().all(&state.db).await?;
        let server_count = all_servers
            .into_iter()
            .filter(|s| {
                if let Some(ref gids_str) = s.group_ids {
                    if let Ok(ids) = serde_json::from_str::<Vec<i32>>(gids_str) {
                        return ids.contains(&g.id);
                    }
                }
                false
            })
            .count();

        list.push(json!({
            "id": g.id,
            "name": g.name,
            "users_count": users_count,
            "server_count": server_count,
            "created_at": g.created_at,
            "updated_at": g.updated_at,
        }));
    }

    Ok(ApiResponse::success(list).into_response())
}

/// POST /api/v2/admin/server/group/save
pub async fn save(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<GroupSaveRequest>,
) -> Result<Response, AppError> {
    if payload.name.trim().is_empty() {
        return Err(AppError::Custom(422, "组名不能为空".to_string()));
    }

    let now = chrono::Utc::now().timestamp();
    if let Some(id) = payload.id {
        let g = ServerGroup::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "组不存在".to_string()))?;

        let mut g_active: server_group::ActiveModel = g.into();
        g_active.name = Set(payload.name);
        g_active.updated_at = Set(now);
        g_active.update(&state.db).await?;
    } else {
        let new_g = server_group::ActiveModel {
            name: Set(payload.name),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_g.insert(&state.db).await?;
    }

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/group/drop
pub async fn drop(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<GroupDropRequest>,
) -> Result<Response, AppError> {
    let g = ServerGroup::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "组不存在".to_string()))?;

    // Check if used by any server
    let all_servers = Server::find().all(&state.db).await?;
    let used_by_server = all_servers.into_iter().any(|s| {
        if let Some(ref gids_str) = s.group_ids {
            if let Ok(ids) = serde_json::from_str::<Vec<i32>>(gids_str) {
                return ids.contains(&payload.id);
            }
        }
        false
    });

    if used_by_server {
        return Err(AppError::Custom(
            400,
            "该组已被节点所使用，无法删除".to_string(),
        ));
    }

    // Check if used by any plan
    let used_by_plan = Plan::find()
        .filter(plan::Column::GroupId.eq(payload.id))
        .one(&state.db)
        .await?;
    if used_by_plan.is_some() {
        return Err(AppError::Custom(
            400,
            "该组已被订阅所使用，无法删除".to_string(),
        ));
    }

    let g_active: server_group::ActiveModel = g.into();
    g_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}
