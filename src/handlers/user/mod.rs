pub mod comm;
pub mod coupon;
pub mod gift_card;
pub mod invite;
pub mod knowledge;
pub mod notice;
pub mod order;
pub mod plan;
pub mod profile;
pub mod server;
pub mod stat;
pub mod telegram;
pub mod ticket;

use axum::{
    routing::{get, post},
    Router,
};

use crate::common::AppState;

pub fn user_routes() -> Router<AppState> {
    Router::new()
        // User profile & security
        .route("/info", get(profile::info))
        .route("/changePassword", post(profile::change_password))
        .route("/update", post(profile::update))
        .route("/resetSecurity", get(profile::reset_security))
        .route("/getSubscribe", get(profile::get_subscribe))
        .route("/getStat", get(profile::get_stat))
        .route("/checkLogin", get(profile::check_login))
        .route("/transfer", post(profile::transfer))
        .route("/getQuickLoginUrl", post(profile::get_quick_login_url))
        .route("/getActiveSession", get(profile::get_active_session))
        .route("/removeActiveSession", post(profile::remove_active_session))
        // Order
        .route("/order/fetch", get(order::fetch))
        .route("/order/detail", get(order::detail))
        .route("/order/save", post(order::save))
        .route("/order/checkout", post(order::checkout))
        .route("/order/check", get(order::check))
        .route("/order/getPaymentMethod", get(order::get_payment_method))
        .route("/order/cancel", post(order::cancel))
        // Plan
        .route("/plan/fetch", get(plan::fetch))
        // Invite
        .route("/invite/save", get(invite::save))
        .route("/invite/fetch", get(invite::fetch))
        .route("/invite/details", get(invite::details))
        // Notice
        .route("/notice/fetch", get(notice::fetch))
        // Ticket
        .route("/ticket/fetch", get(ticket::fetch))
        .route("/ticket/save", post(ticket::save))
        .route("/ticket/reply", post(ticket::reply))
        .route("/ticket/close", post(ticket::close))
        .route("/ticket/withdraw", post(ticket::withdraw))
        // Server
        .route("/server/fetch", get(server::fetch))
        // Coupon
        .route("/coupon/check", post(coupon::check))
        // Gift Card
        .route("/gift-card/check", post(gift_card::check))
        .route("/gift-card/redeem", post(gift_card::redeem))
        .route("/gift-card/history", get(gift_card::history))
        .route("/gift-card/detail", get(gift_card::detail))
        .route("/gift-card/types", get(gift_card::types))
        // Telegram
        .route("/telegram/getBotInfo", get(telegram::get_bot_info))
        // Comm
        .route("/comm/config", get(comm::config))
        .route(
            "/comm/getStripePublicKey",
            post(comm::get_stripe_public_key),
        )
        // Knowledge
        .route("/knowledge/fetch", get(knowledge::fetch))
        .route("/knowledge/getCategory", get(knowledge::get_category))
        // Stat
        .route("/stat/getTrafficLog", get(stat::get_traffic_log))
}
