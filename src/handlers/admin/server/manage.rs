use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{server, server_group, Server, ServerGroup},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct ServerSaveRequest {
    pub id: Option<i32>,
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub code: Option<String>,
    pub parent_id: Option<i32>,
    pub machine_id: Option<i32>,
    pub group_ids: Option<Value>,
    pub route_ids: Option<Value>,
    pub tags: Option<Value>,
    pub host: Option<String>,
    pub port: Option<Value>,
    pub server_port: Option<i32>,
    pub rate: Option<f64>,
    pub rate_time_enable: Option<bool>,
    pub rate_time_ranges: Option<Value>,
    pub protocol_settings: Option<Value>,
    pub custom_outbounds: Option<Value>,
    pub custom_routes: Option<Value>,
    pub cert_config: Option<Value>,
    pub show: Option<Value>,
    pub enabled: Option<bool>,
    pub sort: Option<i32>,
    pub transfer_enable: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ServerUpdateRequest {
    pub id: i32,
    pub show: Option<i32>,
    pub machine_id: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ServerIdRequest {
    pub id: i32,
}

#[derive(Debug, Deserialize)]
pub struct ServerIdsRequest {
    pub ids: Vec<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ServerSortItem {
    pub id: i32,
    pub order: Option<i32>,
}

/// GET /api/v2/admin/server/manage/getNodes
pub async fn get_nodes(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let servers = Server::find()
        .order_by_asc(server::Column::Sort)
        .order_by_asc(server::Column::Id)
        .all(&state.db)
        .await?;

    let mut list = Vec::new();
    for s in servers {
        let mut group_objs = Vec::new();
        if let Some(ref gids_str) = s.group_ids {
            if let Ok(ids) = serde_json::from_str::<Vec<i32>>(gids_str) {
                let groups = ServerGroup::find()
                    .filter(server_group::Column::Id.is_in(ids))
                    .all(&state.db)
                    .await?;
                for g in groups {
                    group_objs.push(json!({ "id": g.id, "name": g.name }));
                }
            }
        }

        let parent = if let Some(pid) = s.parent_id {
            Server::find_by_id(pid).one(&state.db).await?
        } else {
            None
        };

        let is_online = state.server_service.is_node_online(s.id).await;
        let online_users = state.server_service.get_node_online_users(s.id).await;

        let mut val = serde_json::to_value(&s).unwrap_or_default();
        if let Some(obj) = val.as_object_mut() {
            obj.insert("groups".to_string(), json!(group_objs));
            obj.insert("parent".to_string(), json!(parent));
            obj.insert("is_online".to_string(), json!(is_online));
            obj.insert("online_users".to_string(), json!(online_users));
        }
        list.push(val);
    }

    Ok(ApiResponse::success(list).into_response())
}

/// POST /api/v2/admin/server/manage/save
pub async fn save(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<ServerSaveRequest>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();

    let stringify = |v: &Option<Value>| -> Option<String> {
        v.as_ref().map(|val| {
            if let Value::String(s) = val {
                s.clone()
            } else {
                val.to_string()
            }
        })
    };

    let port_str = match &payload.port {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => "443".to_string(),
    };

    if let Some(id) = payload.id {
        let s = Server::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "服务器不存在".to_string()))?;

        let mut s_active: server::ActiveModel = s.into();
        if let Some(name) = payload.name {
            s_active.name = Set(name);
        }
        if let Some(t) = payload.r#type {
            s_active.r#type = Set(t);
        }
        if payload.code.is_some() {
            s_active.code = Set(payload.code);
        }
        if payload.parent_id.is_some() {
            s_active.parent_id = Set(payload.parent_id);
        }
        if payload.machine_id.is_some() {
            s_active.machine_id = Set(payload.machine_id);
        }
        if payload.group_ids.is_some() {
            s_active.group_ids = Set(stringify(&payload.group_ids));
        }
        if payload.route_ids.is_some() {
            s_active.route_ids = Set(stringify(&payload.route_ids));
        }
        if payload.tags.is_some() {
            s_active.tags = Set(stringify(&payload.tags));
        }
        if let Some(host) = payload.host {
            s_active.host = Set(host);
        }
        s_active.port = Set(port_str);
        if let Some(sp) = payload.server_port {
            s_active.server_port = Set(sp);
        }
        if let Some(rate) = payload.rate {
            s_active.rate = Set(rate);
        }
        if let Some(rte) = payload.rate_time_enable {
            s_active.rate_time_enable = Set(rte);
        }
        if payload.rate_time_ranges.is_some() {
            s_active.rate_time_ranges = Set(stringify(&payload.rate_time_ranges));
        }
        if payload.protocol_settings.is_some() {
            s_active.protocol_settings = Set(stringify(&payload.protocol_settings));
        }
        if payload.custom_outbounds.is_some() {
            s_active.custom_outbounds = Set(stringify(&payload.custom_outbounds));
        }
        if payload.custom_routes.is_some() {
            s_active.custom_routes = Set(stringify(&payload.custom_routes));
        }
        if payload.cert_config.is_some() {
            s_active.cert_config = Set(stringify(&payload.cert_config));
        }
        let show_val = payload.show.as_ref().map(|v| match v {
            Value::Bool(b) => *b,
            Value::Number(n) => n.as_i64() != Some(0),
            Value::String(s) => s == "1" || s.to_lowercase() == "true",
            _ => true,
        });

        if let Some(show) = show_val {
            s_active.show = Set(show);
        }
        if let Some(enabled) = payload.enabled {
            s_active.enabled = Set(Some(enabled));
        }
        if payload.sort.is_some() {
            s_active.sort = Set(payload.sort);
        }
        if payload.transfer_enable.is_some() {
            s_active.transfer_enable = Set(payload.transfer_enable);
        }
        s_active.updated_at = Set(now);

        s_active.update(&state.db).await?;
    } else {
        let show_val = payload.show.as_ref().map(|v| match v {
            Value::Bool(b) => *b,
            Value::Number(n) => n.as_i64() != Some(0),
            Value::String(s) => s == "1" || s.to_lowercase() == "true",
            _ => true,
        });

        let new_s = server::ActiveModel {
            name: Set(payload.name.unwrap_or_else(|| "New Server".to_string())),
            r#type: Set(payload.r#type.unwrap_or_else(|| "vless".to_string())),
            code: Set(payload.code),
            parent_id: Set(payload.parent_id),
            machine_id: Set(payload.machine_id),
            group_ids: Set(stringify(&payload.group_ids)),
            route_ids: Set(stringify(&payload.route_ids)),
            tags: Set(stringify(&payload.tags)),
            host: Set(payload.host.unwrap_or_else(|| "127.0.0.1".to_string())),
            port: Set(port_str),
            server_port: Set(payload.server_port.unwrap_or(443)),
            rate: Set(payload.rate.unwrap_or(1.0)),
            rate_time_enable: Set(payload.rate_time_enable.unwrap_or(false)),
            rate_time_ranges: Set(stringify(&payload.rate_time_ranges)),
            protocol_settings: Set(stringify(&payload.protocol_settings)),
            custom_outbounds: Set(stringify(&payload.custom_outbounds)),
            custom_routes: Set(stringify(&payload.custom_routes)),
            cert_config: Set(stringify(&payload.cert_config)),
            show: Set(show_val.unwrap_or(true)),
            enabled: Set(payload.enabled.or(Some(true))),
            sort: Set(payload.sort),
            transfer_enable: Set(payload.transfer_enable),
            u: Set(Some(0)),
            d: Set(Some(0)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        new_s.insert(&state.db).await?;
    }

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/manage/update
pub async fn update(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<ServerUpdateRequest>,
) -> Result<Response, AppError> {
    let s = Server::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "服务器不存在".to_string()))?;

    let mut s_active: server::ActiveModel = s.into();
    if let Some(show) = payload.show {
        s_active.show = Set(show != 0);
    }
    if let Some(mid) = payload.machine_id {
        s_active.machine_id = Set(if mid == 0 { None } else { Some(mid) });
    }
    if let Some(enabled) = payload.enabled {
        s_active.enabled = Set(Some(enabled));
    }
    s_active.updated_at = Set(chrono::Utc::now().timestamp());
    s_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/manage/drop
pub async fn drop(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<ServerIdRequest>,
) -> Result<Response, AppError> {
    let s = Server::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "服务器不存在".to_string()))?;

    let s_active: server::ActiveModel = s.into();
    s_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/manage/copy
pub async fn copy(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<ServerIdRequest>,
) -> Result<Response, AppError> {
    let s = Server::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "服务器不存在".to_string()))?;

    let now = chrono::Utc::now().timestamp();
    let copied = server::ActiveModel {
        name: Set(format!("{} (副本)", s.name)),
        r#type: Set(s.r#type),
        code: Set(s.code),
        parent_id: Set(s.parent_id),
        machine_id: Set(s.machine_id),
        group_ids: Set(s.group_ids),
        route_ids: Set(s.route_ids),
        tags: Set(s.tags),
        host: Set(s.host),
        port: Set(s.port),
        server_port: Set(s.server_port),
        rate: Set(s.rate),
        rate_time_enable: Set(s.rate_time_enable),
        rate_time_ranges: Set(s.rate_time_ranges),
        protocol_settings: Set(s.protocol_settings),
        custom_outbounds: Set(s.custom_outbounds),
        custom_routes: Set(s.custom_routes),
        cert_config: Set(s.cert_config),
        show: Set(s.show),
        enabled: Set(s.enabled),
        sort: Set(s.sort),
        transfer_enable: Set(s.transfer_enable),
        u: Set(Some(0)),
        d: Set(Some(0)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    copied.insert(&state.db).await?;
    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/manage/sort
pub async fn sort(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<Value>,
) -> Result<Response, AppError> {
    let txn = state.db.begin().await?;

    if let Some(arr) = payload.as_array() {
        for item in arr {
            if let (Some(id), Some(order)) = (
                item.get("id").and_then(|v| v.as_i64()),
                item.get("order").and_then(|v| v.as_i64()),
            ) {
                if let Some(s) = Server::find_by_id(id as i32).one(&txn).await? {
                    let mut s_active: server::ActiveModel = s.into();
                    s_active.sort = Set(Some(order as i32));
                    s_active.update(&txn).await?;
                }
            }
        }
    }
    txn.commit().await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/manage/batchDelete
pub async fn batch_delete(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<ServerIdsRequest>,
) -> Result<Response, AppError> {
    if payload.ids.is_empty() {
        return Err(AppError::Custom(400, "请选择要删除的节点".to_string()));
    }

    Server::delete_many()
        .filter(server::Column::Id.is_in(payload.ids))
        .exec(&state.db)
        .await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/manage/resetTraffic
pub async fn reset_traffic(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<ServerIdRequest>,
) -> Result<Response, AppError> {
    let s = Server::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "服务器不存在".to_string()))?;

    let mut s_active: server::ActiveModel = s.into();
    s_active.u = Set(Some(0));
    s_active.d = Set(Some(0));
    s_active.updated_at = Set(chrono::Utc::now().timestamp());
    s_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/manage/batchResetTraffic
pub async fn batch_reset_traffic(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<ServerIdsRequest>,
) -> Result<Response, AppError> {
    let servers = Server::find()
        .filter(server::Column::Id.is_in(payload.ids))
        .all(&state.db)
        .await?;

    let now = chrono::Utc::now().timestamp();
    let txn = state.db.begin().await?;
    for s in servers {
        let mut s_active: server::ActiveModel = s.into();
        s_active.u = Set(Some(0));
        s_active.d = Set(Some(0));
        s_active.updated_at = Set(now);
        s_active.update(&txn).await?;
    }
    txn.commit().await?;

    Ok(ApiResponse::success(true).into_response())
}

/// GET /api/v2/admin/server/manage/generateEchKey
pub async fn generate_ech_key(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use rand::RngCore;

    let mut priv_key = [0u8; 32];
    let mut pub_key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut priv_key);
    rand::thread_rng().fill_bytes(&mut pub_key);

    let res = json!({
        "private_key": BASE64.encode(priv_key),
        "public_key": BASE64.encode(pub_key)
    });

    Ok(ApiResponse::success(res).into_response())
}
