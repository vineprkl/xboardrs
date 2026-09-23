use axum::{
    extract::{Json, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

use crate::{
    common::{AppError, AppState},
    handlers::server::auth::{AuthenticatedMachine, AuthenticatedNode, NodeOrMachineAuth},
};

#[derive(Debug, Deserialize, Default)]
pub struct V2ReportPayload {
    pub traffic: Option<HashMap<String, [i64; 2]>>,
    pub alive: Option<HashMap<String, Vec<String>>>,
    pub status: Option<serde_json::Value>,
    pub metrics: Option<serde_json::Value>,
}

/// GET / POST /api/v2/server/handshake
pub async fn handshake(
    State(state): State<AppState>,
    _auth: NodeOrMachineAuth,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let ws_enable = state
        .setting_service
        .get_bool("server_ws_enable", false)
        .await;

    let websocket = if ws_enable {
        let custom_url = state
            .setting_service
            .get_string("server_ws_url", "")
            .await
            .trim()
            .to_string();

        let ws_url = if !custom_url.is_empty() {
            custom_url.trim_end_matches('/').to_string()
        } else {
            let host = headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("localhost");
            format!("ws://{}/ws", host)
        };

        json!({
            "enabled": true,
            "ws_url": ws_url,
        })
    } else {
        json!({
            "enabled": false,
        })
    };

    Ok(Json(json!({ "websocket": websocket })).into_response())
}

/// POST /api/v2/server/report
pub async fn report(
    State(state): State<AppState>,
    auth: AuthenticatedNode,
    Json(payload): Json<V2ReportPayload>,
) -> Result<Response, AppError> {
    state.server_service.touch_node(auth.server.id).await;

    if let Some(traffic_map) = payload.traffic {
        let traffic: HashMap<i32, [i64; 2]> = traffic_map
            .into_iter()
            .filter_map(|(k, v)| k.parse::<i32>().ok().map(|uid| (uid, v)))
            .collect();
        state
            .server_service
            .process_traffic(&auth.server, traffic)
            .await?;
    }

    if let Some(alive_map) = payload.alive {
        let alive: HashMap<i32, Vec<String>> = alive_map
            .into_iter()
            .filter_map(|(k, v)| k.parse::<i32>().ok().map(|uid| (uid, v)))
            .collect();
        state
            .server_service
            .process_alive(auth.server.id, alive)
            .await;
    }

    if let Some(status_val) = payload.status {
        state
            .server_service
            .process_status(auth.server.id, status_val)
            .await;
    }

    if let Some(metrics_val) = payload.metrics {
        state
            .server_service
            .update_metrics(auth.server.id, metrics_val)
            .await;
    }

    Ok(Json(json!({ "data": true })).into_response())
}

/// POST /api/v2/server/machine/nodes
pub async fn machine_nodes(
    State(state): State<AppState>,
    auth: AuthenticatedMachine,
) -> Result<Response, AppError> {
    let nodes = state
        .server_service
        .get_machine_nodes(&auth.machine)
        .await?;

    let node_list: Vec<_> = nodes
        .into_iter()
        .map(|n| {
            json!({
                "id": n.id,
                "type": n.r#type,
                "name": n.name,
            })
        })
        .collect();

    let push_interval = state
        .setting_service
        .get_int("server_push_interval", 60)
        .await;
    let pull_interval = state
        .setting_service
        .get_int("server_pull_interval", 60)
        .await;

    Ok(Json(json!({
        "nodes": node_list,
        "base_config": {
            "push_interval": push_interval,
            "pull_interval": pull_interval,
        }
    }))
    .into_response())
}

/// POST /api/v2/server/machine/status
pub async fn machine_status(
    State(state): State<AppState>,
    auth: AuthenticatedMachine,
    Json(payload): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    let net_in = payload
        .get("net")
        .and_then(|n| n.get("in_speed"))
        .and_then(|v| v.as_f64());
    let net_out = payload
        .get("net")
        .and_then(|n| n.get("out_speed"))
        .and_then(|v| v.as_f64());

    state
        .server_service
        .process_machine_status(&auth.machine, payload, net_in, net_out)
        .await?;

    Ok(Json(json!({ "data": true })).into_response())
}
