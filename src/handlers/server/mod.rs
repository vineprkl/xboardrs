pub mod auth;
pub mod uniproxy;
pub mod v2_server;

use axum::{
    routing::{get, post},
    Router,
};

use crate::common::AppState;

pub fn server_router() -> Router<AppState> {
    Router::new()
        // V1 UniProxy routes
        .route("/api/v1/server/UniProxy/config", get(uniproxy::config))
        .route("/api/v1/server/UniProxy/user", get(uniproxy::user))
        .route("/api/v1/server/UniProxy/push", post(uniproxy::push))
        .route("/api/v1/server/UniProxy/alive", post(uniproxy::alive))
        .route(
            "/api/v1/server/UniProxy/alivelist",
            get(uniproxy::alivelist),
        )
        .route("/api/v1/server/UniProxy/status", post(uniproxy::status))
        // Direct /server/UniProxy/* for reverse proxies
        .route("/server/UniProxy/config", get(uniproxy::config))
        .route("/server/UniProxy/user", get(uniproxy::user))
        .route("/server/UniProxy/push", post(uniproxy::push))
        .route("/server/UniProxy/alive", post(uniproxy::alive))
        .route("/server/UniProxy/alivelist", get(uniproxy::alivelist))
        .route("/server/UniProxy/status", post(uniproxy::status))
        // V2 server routes
        .route(
            "/api/v2/server/handshake",
            get(v2_server::handshake).post(v2_server::handshake),
        )
        .route("/api/v2/server/report", post(v2_server::report))
        .route("/api/v2/server/config", get(uniproxy::config))
        .route("/api/v2/server/user", get(uniproxy::user))
        .route("/api/v2/server/push", post(uniproxy::push))
        .route("/api/v2/server/alive", post(uniproxy::alive))
        .route("/api/v2/server/alivelist", get(uniproxy::alivelist))
        .route("/api/v2/server/status", post(uniproxy::status))
        .route(
            "/api/v2/server/machine/nodes",
            post(v2_server::machine_nodes),
        )
        .route(
            "/api/v2/server/machine/status",
            post(v2_server::machine_status),
        )
        // Direct /server/* (V2)
        .route(
            "/server/handshake",
            get(v2_server::handshake).post(v2_server::handshake),
        )
        .route("/server/report", post(v2_server::report))
        .route("/server/config", get(uniproxy::config))
        .route("/server/user", get(uniproxy::user))
        .route("/server/push", post(uniproxy::push))
        .route("/server/alive", post(uniproxy::alive))
        .route("/server/alivelist", get(uniproxy::alivelist))
        .route("/server/status", post(uniproxy::status))
        .route("/server/machine/nodes", post(v2_server::machine_nodes))
        .route("/server/machine/status", post(v2_server::machine_status))
}
