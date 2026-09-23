pub mod auth;
pub mod uniproxy;
pub mod v2_server;

use axum::{
    body::Body,
    extract::Request,
    http::header::HeaderValue,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};

use crate::common::AppState;

/// Middleware for Server & Node API routes.
/// When xboard-node sends POST requests (such as machine/nodes, machine/status, handshake, report, push, alive),
/// it includes authentication fields ("token", "machine_id", "node_id", "node_type") directly in the JSON body
/// rather than in query parameters or custom headers.
/// This middleware buffers the request body, extracts those fields, and injects them into request headers
/// so that downstream Axum `FromRequestParts` extractors (AuthenticatedMachine, AuthenticatedNode, NodeOrMachineAuth)
/// can seamlessly authenticate the request while preserving the raw body for route handlers.
async fn server_body_auth_middleware(req: Request, next: Next) -> Response {
    let has_token = req.headers().contains_key("token")
        || req.headers().contains_key("authorization")
        || req
            .uri()
            .query()
            .map(|q| q.contains("token="))
            .unwrap_or(false);

    let has_machine_id = req.headers().contains_key("machine-id")
        || req.headers().contains_key("machine_id")
        || req
            .uri()
            .query()
            .map(|q| q.contains("machine_id="))
            .unwrap_or(false);

    let has_node_id = req.headers().contains_key("node-id")
        || req.headers().contains_key("node_id")
        || req
            .uri()
            .query()
            .map(|q| q.contains("node_id="))
            .unwrap_or(false);

    if has_token && (has_machine_id || has_node_id) {
        return next.run(req).await;
    }

    let method = req.method();
    if method == axum::http::Method::POST
        || method == axum::http::Method::PUT
        || method == axum::http::Method::PATCH
    {
        let (mut parts, body) = req.into_parts();
        if let Ok(bytes) = axum::body::to_bytes(body, 10 * 1024 * 1024).await {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if !has_token {
                    if let Some(token) = json.get("token").and_then(|v| v.as_str()) {
                        if let Ok(hv) = HeaderValue::from_str(token) {
                            parts.headers.insert("token", hv);
                        }
                    }
                }
                if !has_machine_id {
                    if let Some(mid) = json.get("machine_id").or_else(|| json.get("machine-id")) {
                        let s = match mid {
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            _ => String::new(),
                        };
                        if !s.is_empty() {
                            if let Ok(hv) = HeaderValue::from_str(&s) {
                                parts.headers.insert("machine-id", hv);
                            }
                        }
                    }
                }
                if !has_node_id {
                    if let Some(nid) = json.get("node_id").or_else(|| json.get("node-id")) {
                        let s = match nid {
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            _ => String::new(),
                        };
                        if !s.is_empty() {
                            if let Ok(hv) = HeaderValue::from_str(&s) {
                                parts.headers.insert("node-id", hv);
                            }
                        }
                    }
                }
                if let Some(nt) = json
                    .get("node_type")
                    .or_else(|| json.get("node-type"))
                    .and_then(|v| v.as_str())
                {
                    if let Ok(hv) = HeaderValue::from_str(nt) {
                        parts.headers.insert("node-type", hv);
                    }
                }
            }
            let req = Request::from_parts(parts, Body::from(bytes));
            return next.run(req).await;
        } else {
            let req = Request::from_parts(parts, Body::empty());
            return next.run(req).await;
        }
    }

    next.run(req).await
}

pub fn server_router() -> Router<AppState> {
    Router::new()
        // V1 UniProxy routes
        .route(
            "/api/v1/server/UniProxy/config",
            get(uniproxy::config).post(uniproxy::config),
        )
        .route(
            "/api/v1/server/UniProxy/user",
            get(uniproxy::user).post(uniproxy::user),
        )
        .route("/api/v1/server/UniProxy/push", post(uniproxy::push))
        .route("/api/v1/server/UniProxy/alive", post(uniproxy::alive))
        .route(
            "/api/v1/server/UniProxy/alivelist",
            get(uniproxy::alivelist).post(uniproxy::alivelist),
        )
        .route("/api/v1/server/UniProxy/status", post(uniproxy::status))
        // Direct /server/UniProxy/* for reverse proxies
        .route(
            "/server/UniProxy/config",
            get(uniproxy::config).post(uniproxy::config),
        )
        .route(
            "/server/UniProxy/user",
            get(uniproxy::user).post(uniproxy::user),
        )
        .route("/server/UniProxy/push", post(uniproxy::push))
        .route("/server/UniProxy/alive", post(uniproxy::alive))
        .route(
            "/server/UniProxy/alivelist",
            get(uniproxy::alivelist).post(uniproxy::alivelist),
        )
        .route("/server/UniProxy/status", post(uniproxy::status))
        // V2 server routes
        .route(
            "/api/v2/server/handshake",
            get(v2_server::handshake).post(v2_server::handshake),
        )
        .route("/api/v2/server/report", post(v2_server::report))
        .route(
            "/api/v2/server/config",
            get(uniproxy::config).post(uniproxy::config),
        )
        .route(
            "/api/v2/server/user",
            get(uniproxy::user).post(uniproxy::user),
        )
        .route("/api/v2/server/push", post(uniproxy::push))
        .route("/api/v2/server/alive", post(uniproxy::alive))
        .route(
            "/api/v2/server/alivelist",
            get(uniproxy::alivelist).post(uniproxy::alivelist),
        )
        .route("/api/v2/server/status", post(uniproxy::status))
        .route(
            "/api/v2/server/machine/nodes",
            post(v2_server::machine_nodes).get(v2_server::machine_nodes),
        )
        .route(
            "/api/v2/server/machine/status",
            post(v2_server::machine_status).get(v2_server::machine_status),
        )
        // Direct /server/* (V2)
        .route(
            "/server/handshake",
            get(v2_server::handshake).post(v2_server::handshake),
        )
        .route("/server/report", post(v2_server::report))
        .route(
            "/server/config",
            get(uniproxy::config).post(uniproxy::config),
        )
        .route("/server/user", get(uniproxy::user).post(uniproxy::user))
        .route("/server/push", post(uniproxy::push))
        .route("/server/alive", post(uniproxy::alive))
        .route(
            "/server/alivelist",
            get(uniproxy::alivelist).post(uniproxy::alivelist),
        )
        .route("/server/status", post(uniproxy::status))
        .route(
            "/server/machine/nodes",
            post(v2_server::machine_nodes).get(v2_server::machine_nodes),
        )
        .route(
            "/server/machine/status",
            post(v2_server::machine_status).get(v2_server::machine_status),
        )
        .layer(middleware::from_fn(server_body_auth_middleware))
}
