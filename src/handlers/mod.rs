pub mod admin;
pub mod auth;
pub mod client;
pub mod guest;
pub mod health;
pub mod passport;
pub mod server;
pub mod user;
pub mod web;

pub use auth::{AuthenticatedAdmin, AuthenticatedUser};

use axum::{
    extract::State,
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower::ServiceExt;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::common::AppState;

async fn dynamic_fallback(
    State(state): State<AppState>,
    mut req: axum::extract::Request,
) -> Response {
    let path = req.uri().path().to_string();
    let trimmed = path.trim_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();

    // 1. Dynamic subscription path: /{subscribe_path}/{token}
    if parts.len() == 2 {
        let configured_path = state
            .setting_service
            .get_string("subscribe_path", "s")
            .await;
        if parts[0] == configured_path {
            let headers = req.headers().clone();
            let query: client::subscribe::SubscribeQuery =
                axum::extract::Query::try_from_uri(req.uri())
                    .map(|q| q.0)
                    .unwrap_or_default();
            return match client::subscribe::handle_subscribe(&state, parts[1], &query, &headers)
                .await
            {
                Ok(resp) => resp,
                Err(err) => err.into_response(),
            };
        }
    }

    // 2. Dynamic admin secure path: /api/v2/{secure_path}/*
    if parts.len() >= 3 && parts[0] == "api" && parts[1] == "v2" {
        let secure_path = state.setting_service.get_string("secure_path", "").await;
        let frontend_admin_path = state
            .setting_service
            .get_string("frontend_admin_path", "")
            .await;
        let app_key = std::env::var("APP_KEY")
            .unwrap_or_else(|_| "base64:xboard_default_key_32bytes!!".to_string());
        let fallback_crc = crate::utils::crc32b(app_key.as_bytes());

        let matches_admin = (!secure_path.is_empty() && parts[2] == secure_path)
            || (!frontend_admin_path.is_empty() && parts[2] == frontend_admin_path)
            || parts[2] == fallback_crc
            || parts[2] == "admin";

        if matches_admin {
            let sub_parts: Vec<&str> = parts[3..]
                .iter()
                .copied()
                .filter(|s| !s.is_empty())
                .collect();
            let sub_path = format!("/{}", sub_parts.join("/"));
            let query = req
                .uri()
                .query()
                .map(|q| format!("?{}", q))
                .unwrap_or_default();
            if let Ok(new_uri) = format!("{}{}", sub_path, query).parse::<Uri>() {
                *req.uri_mut() = new_uri;
                return match admin::admin_routes().with_state(state).oneshot(req).await {
                    Ok(resp) => resp,
                    Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                };
            }
        }
    }

    // 3. API requests that didn't match should return 404 NOT_FOUND (not HTML)
    if path.starts_with("/api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    // 4. Web & SPA fallback for frontends and static files
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    drop(req);
    web::web_fallback_handler(&state, &method, &uri, &headers).await
}

pub fn app_router_with_state(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let admin_router = admin::admin_routes();

    Router::new()
        .route("/health", get(health::health_check))
        .route("/s/{token}", get(client::subscribe::subscribe_by_path))
        .route("/sub/{token}", get(client::subscribe::subscribe_by_path))
        .nest("/api/v1/client", client::client_router())
        .nest("/api/v2/client", client::client_router())
        .nest("/api/v1/guest", guest::guest_router())
        .nest("/api/v1/passport", passport::passport_router())
        .nest("/api/v2/passport", passport::passport_router())
        .nest("/api/v1/user", user::user_routes())
        .nest("/api/v2/user", user::user_routes())
        .nest("/api/v2/admin", admin_router)
        .merge(server::server_router())
        .fallback(dynamic_fallback)
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

pub fn app_router() -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health::health_check))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
