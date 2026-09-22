use axum::{
    extract::{Query, State},
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
    common::{ApiResponse, AppError, AppState},
    entities::{
        server, server_machine, server_machine_load_history, Server, ServerMachine,
        ServerMachineLoadHistory,
    },
    handlers::auth::AuthenticatedAdmin,
    utils::random_char,
};

#[derive(Debug, Deserialize)]
pub struct MachineQuery {
    pub id: Option<i32>,
    pub machine_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct MachineSaveRequest {
    pub id: Option<i32>,
    pub name: String,
    pub token: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MachineIdRequest {
    pub id: i32,
}

/// GET /api/v2/admin/server/machine/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let machines = ServerMachine::find()
        .order_by_desc(server_machine::Column::Id)
        .all(&state.db)
        .await?;

    let mut list = Vec::new();
    for m in machines {
        let node_count = Server::find()
            .filter(server::Column::MachineId.eq(m.id))
            .count(&state.db)
            .await?;

        let mut val = serde_json::to_value(&m).unwrap_or_default();
        if let Some(obj) = val.as_object_mut() {
            obj.insert("node_count".to_string(), json!(node_count));
        }
        list.push(val);
    }

    Ok(ApiResponse::success(list).into_response())
}

async fn resolve_panel_url(state: &AppState, headers: &axum::http::HeaderMap) -> String {
    let app_url = state.setting_service.get_string("app_url", "").await;
    if !app_url.trim().is_empty() {
        return app_url.trim().trim_end_matches('/').to_string();
    }
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1:7001");
    let scheme = if headers
        .get("x-forwarded-proto")
        .and_then(|p| p.to_str().ok())
        == Some("https")
    {
        "https"
    } else {
        "http"
    };
    format!("{}://{}", scheme, host)
}

fn build_install_command(panel_url: &str, machine: &server_machine::Model) -> String {
    let panel = panel_url.trim_end_matches('/');
    let installer_url = "https://raw.githubusercontent.com/cedar2025/xboard-node/dev/install.sh";
    format!(
        "curl -fsSL {} | sudo bash -s -- --mode machine --panel '{}' --token '{}' --machine-id {}",
        installer_url, panel, machine.token, machine.id
    )
}

/// POST /api/v2/admin/server/machine/save
pub async fn save(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    headers: axum::http::HeaderMap,
    Json(payload): Json<MachineSaveRequest>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();

    if let Some(id) = payload.id {
        let m = ServerMachine::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "机器不存在".to_string()))?;

        let mut m_active: server_machine::ActiveModel = m.into();
        m_active.name = Set(payload.name);
        if let Some(token) = payload.token {
            m_active.token = Set(token);
        }
        if let Some(notes) = payload.notes {
            m_active.notes = Set(Some(notes));
        }
        m_active.updated_at = Set(now);
        m_active.update(&state.db).await?;
        Ok(ApiResponse::success(true).into_response())
    } else {
        let token = payload.token.unwrap_or_else(|| random_char(32, false));
        let new_m = server_machine::ActiveModel {
            name: Set(payload.name),
            token: Set(token),
            notes: Set(payload.notes),
            is_active: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let inserted = new_m.insert(&state.db).await?;
        let panel_url = resolve_panel_url(&state, &headers).await;
        let install_cmd = build_install_command(&panel_url, &inserted);

        let data = json!({
            "id": inserted.id,
            "token": inserted.token,
            "install_command": install_cmd,
        });
        Ok(ApiResponse::success(data).into_response())
    }
}

/// POST /api/v2/admin/server/machine/drop
pub async fn drop(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<MachineIdRequest>,
) -> Result<Response, AppError> {
    let m = ServerMachine::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "机器不存在".to_string()))?;

    // Clear machine_id on servers
    let servers = Server::find()
        .filter(server::Column::MachineId.eq(payload.id))
        .all(&state.db)
        .await?;
    for s in servers {
        let mut s_active: server::ActiveModel = s.into();
        s_active.machine_id = Set(None);
        s_active.update(&state.db).await?;
    }

    let m_active: server_machine::ActiveModel = m.into();
    m_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/server/machine/resetToken
pub async fn reset_token(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<MachineIdRequest>,
) -> Result<Response, AppError> {
    let m = ServerMachine::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "机器不存在".to_string()))?;

    let new_token = random_char(32, false);
    let mut m_active: server_machine::ActiveModel = m.into();
    m_active.token = Set(new_token.clone());
    m_active.updated_at = Set(chrono::Utc::now().timestamp());
    m_active.update(&state.db).await?;

    let data = json!({
        "token": new_token,
    });
    Ok(ApiResponse::success(data).into_response())
}

/// GET /api/v2/admin/server/machine/getToken
pub async fn get_token(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<MachineQuery>,
) -> Result<Response, AppError> {
    let id = query
        .id
        .or(query.machine_id)
        .ok_or_else(|| AppError::Custom(422, "id is required".to_string()))?;

    let m = ServerMachine::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "机器不存在".to_string()))?;

    let data = json!({
        "token": m.token,
    });
    Ok(ApiResponse::success(data).into_response())
}

/// GET /api/v2/admin/server/machine/installCommand
pub async fn install_command(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    headers: axum::http::HeaderMap,
    Query(query): Query<MachineQuery>,
) -> Result<Response, AppError> {
    let id = query
        .id
        .or(query.machine_id)
        .ok_or_else(|| AppError::Custom(422, "id is required".to_string()))?;

    let m = ServerMachine::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "机器不存在".to_string()))?;

    let panel_url = resolve_panel_url(&state, &headers).await;
    let cmd = build_install_command(&panel_url, &m);

    let data = json!({
        "command": cmd,
    });

    Ok(ApiResponse::success(data).into_response())
}

/// GET /api/v2/admin/server/machine/nodes
pub async fn nodes(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<MachineQuery>,
) -> Result<Response, AppError> {
    let id = query
        .id
        .or(query.machine_id)
        .ok_or_else(|| AppError::Custom(422, "id is required".to_string()))?;

    let servers = Server::find()
        .filter(server::Column::MachineId.eq(id))
        .all(&state.db)
        .await?;

    Ok(ApiResponse::success(servers).into_response())
}

/// GET /api/v2/admin/server/machine/history
pub async fn history(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<MachineQuery>,
) -> Result<Response, AppError> {
    let id = query
        .id
        .or(query.machine_id)
        .ok_or_else(|| AppError::Custom(422, "id is required".to_string()))?;

    let history = ServerMachineLoadHistory::find()
        .filter(server_machine_load_history::Column::MachineId.eq(id))
        .order_by_desc(server_machine_load_history::Column::CreatedAt)
        .limit(50)
        .all(&state.db)
        .await?;

    Ok(ApiResponse::success(history).into_response())
}
