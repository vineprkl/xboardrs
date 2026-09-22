pub mod comm;
pub mod payment;
pub mod plan;

use crate::common::AppState;
use axum::{routing::get, Router};

/// Creates the router for Guest API endpoints (`/api/v1/guest`).
pub fn guest_router() -> Router<AppState> {
    Router::new()
        .route("/comm/config", get(comm::config))
        .route("/plan/fetch", get(plan::fetch))
        .route(
            "/payment/notify/{method}/{uuid}",
            get(payment::notify).post(payment::notify),
        )
}
