use axum::{
    extract::{Json, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::collections::HashMap;

use crate::{
    common::{ApiResponse, AppError, AppState},
    handlers::server::auth::AuthenticatedNode,
    utils::sha1_hex,
};

/// GET /server/UniProxy/config or /api/v1/server/UniProxy/config
pub async fn config(
    State(state): State<AppState>,
    auth: AuthenticatedNode,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let node_config = state.server_service.build_node_config(&auth.server).await?;
    let body_str = serde_json::to_string(&node_config)
        .map_err(|e| AppError::Internal(format!("Serialization failed: {}", e)))?;
    let etag = format!("\"{}\"", sha1_hex(body_str.as_bytes()));

    if let Some(if_none_match) = headers.get("if-none-match").and_then(|v| v.to_str().ok()) {
        if if_none_match.contains(&etag) || if_none_match.contains(&etag.replace('"', "")) {
            return Ok((
                StatusCode::NOT_MODIFIED,
                [(header::ETAG, etag.as_str())],
                "",
            )
                .into_response());
        }
    }

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ETAG, etag.as_str()),
        ],
        body_str,
    )
        .into_response())
}

/// GET /server/UniProxy/user or /api/v1/server/UniProxy/user
pub async fn user(
    State(state): State<AppState>,
    auth: AuthenticatedNode,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    state.server_service.touch_node(auth.server.id).await;
    let users = state
        .server_service
        .get_available_users(&auth.server)
        .await?;
    let response_data = json!({ "users": users });
    let body_str = serde_json::to_string(&response_data)
        .map_err(|e| AppError::Internal(format!("Serialization failed: {}", e)))?;
    let etag = format!("\"{}\"", sha1_hex(body_str.as_bytes()));

    if let Some(if_none_match) = headers.get("if-none-match").and_then(|v| v.to_str().ok()) {
        if if_none_match.contains(&etag) || if_none_match.contains(&etag.replace('"', "")) {
            return Ok((
                StatusCode::NOT_MODIFIED,
                [(header::ETAG, etag.as_str())],
                "",
            )
                .into_response());
        }
    }

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ETAG, etag.as_str()),
        ],
        body_str,
    )
        .into_response())
}

/// POST /server/UniProxy/push or /api/v1/server/UniProxy/push
pub async fn push(
    State(state): State<AppState>,
    auth: AuthenticatedNode,
    Json(payload): Json<HashMap<String, [i64; 2]>>,
) -> Result<Response, AppError> {
    let traffic: HashMap<i32, [i64; 2]> = payload
        .into_iter()
        .filter_map(|(k, v)| k.parse::<i32>().ok().map(|uid| (uid, v)))
        .collect();

    state
        .server_service
        .process_traffic(&auth.server, traffic)
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}

/// POST /server/UniProxy/alive or /api/v1/server/UniProxy/alive
pub async fn alive(
    State(state): State<AppState>,
    auth: AuthenticatedNode,
    Json(payload): Json<HashMap<String, Vec<String>>>,
) -> Result<Response, AppError> {
    let alive: HashMap<i32, Vec<String>> = payload
        .into_iter()
        .filter_map(|(k, v)| k.parse::<i32>().ok().map(|uid| (uid, v)))
        .collect();

    state
        .server_service
        .process_alive(auth.server.id, alive)
        .await;

    Ok(Json(json!({ "data": true })).into_response())
}

/// GET /server/UniProxy/alivelist or /api/v1/server/UniProxy/alivelist
pub async fn alivelist(
    State(state): State<AppState>,
    auth: AuthenticatedNode,
) -> Result<Response, AppError> {
    let users = state
        .server_service
        .get_available_users(&auth.server)
        .await?;
    let user_ids: Vec<i32> = users
        .into_iter()
        .filter(|u| u.device_limit.unwrap_or(0) > 0)
        .map(|u| u.id)
        .collect();

    let alive_map = state.device_state_service.get_alive_list(&user_ids).await;
    Ok(Json(json!({ "alive": alive_map })).into_response())
}

/// POST /server/UniProxy/status or /api/v1/server/UniProxy/status
pub async fn status(
    State(state): State<AppState>,
    auth: AuthenticatedNode,
    Json(payload): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    state
        .server_service
        .process_status(auth.server.id, payload)
        .await;

    Ok(Json(json!({
        "data": true,
        "code": 0,
        "message": "success"
    }))
    .into_response())
}
