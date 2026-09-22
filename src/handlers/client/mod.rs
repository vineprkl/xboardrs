pub mod app;
pub mod subscribe;

use crate::common::AppState;
use axum::{routing::get, Router};

/// Creates the router for Client API endpoints (`/api/v1/client` and `/api/v2/client`).
pub fn client_router() -> Router<AppState> {
    Router::new()
        .route("/subscribe", get(subscribe::subscribe_legacy))
        .route("/app/getConfig", get(app::get_config))
        .route("/app/getVersion", get(app::get_version))
}
