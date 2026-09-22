pub mod auth;
pub mod comm;

use crate::common::AppState;
use axum::{
    routing::{get, post},
    Router,
};

/// Creates the router for Passport API endpoints (`/api/v1/passport` and `/api/v2/passport`).
pub fn passport_router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/token2Login", get(auth::token_to_login))
        .route("/auth/forget", post(auth::forget))
        .route("/auth/getQuickLoginUrl", post(auth::get_quick_login_url))
        .route("/auth/loginWithMailLink", post(auth::login_with_mail_link))
        .route("/comm/sendEmailVerify", post(comm::send_email_verify))
        .route("/comm/pv", post(comm::pv))
}
