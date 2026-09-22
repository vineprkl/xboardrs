pub mod config;
pub mod coupon;
pub mod gift_card;
pub mod knowledge;
pub mod mail_template;
pub mod notice;
pub mod order;
pub mod payment;
pub mod plan;
pub mod plugin;
pub mod server;
pub mod stat;
pub mod system;
pub mod theme;
pub mod ticket;
pub mod traffic_reset;
pub mod user;

use crate::common::AppState;
use axum::routing::{get, post};
use axum::Router;

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        // Config
        .route("/config/fetch", get(config::fetch))
        .route("/config/save", post(config::save))
        .route("/config/getEmailTemplate", get(config::get_email_template))
        .route("/config/getThemeTemplate", get(config::get_theme_template))
        .route(
            "/config/setTelegramWebhook",
            post(config::set_telegram_webhook),
        )
        .route("/config/testSendMail", post(config::test_send_mail))
        // Plan
        .route("/plan/fetch", get(plan::fetch))
        .route("/plan/save", post(plan::save))
        .route("/plan/drop", post(plan::drop))
        .route("/plan/update", post(plan::update))
        .route("/plan/sort", post(plan::sort))
        // Server
        .nest("/server", server::router())
        // Order
        .route("/order/fetch", get(order::fetch).post(order::fetch))
        .route("/order/detail", post(order::detail))
        .route("/order/paid", post(order::paid))
        .route("/order/cancel", post(order::cancel))
        .route("/order/update", post(order::update))
        .route("/order/assign", post(order::assign))
        // User
        .route("/user/fetch", get(user::fetch).post(user::fetch))
        .route("/user/getUserInfoById", get(user::get_user_info_by_id))
        .route("/user/update", post(user::update))
        .route("/user/generate", post(user::generate))
        .route("/user/ban", post(user::ban))
        .route("/user/resetSecret", post(user::reset_secret))
        .route("/user/setInviteUser", post(user::set_invite_user))
        .route("/user/destroy", post(user::destroy))
        .route("/user/dumpCSV", post(user::dump_csv))
        .route("/user/sendMail", post(user::send_mail))
        // Stat
        .route("/stat/getOverride", get(stat::get_override))
        .route("/stat/getStats", get(stat::get_stats))
        .route("/stat/getRanking", get(stat::get_ranking))
        .route("/stat/getServerLastRank", get(stat::get_server_last_rank))
        .route(
            "/stat/getServerYesterdayRank",
            get(stat::get_server_yesterday_rank),
        )
        .route("/stat/getTrafficRank", get(stat::get_traffic_rank))
        .route("/stat/getOrder", get(stat::get_order))
        .route(
            "/stat/getStatUser",
            get(stat::get_stat_user).post(stat::get_stat_user),
        )
        .route("/stat/getStatRecord", get(stat::get_stat_record))
        // Notice
        .route("/notice/fetch", get(notice::fetch))
        .route("/notice/save", post(notice::save))
        .route("/notice/show", post(notice::show))
        .route("/notice/drop", post(notice::drop))
        .route("/notice/sort", post(notice::sort))
        // Ticket
        .route("/ticket/fetch", get(ticket::fetch).post(ticket::fetch))
        .route("/ticket/reply", post(ticket::reply))
        .route("/ticket/close", post(ticket::close))
        // Coupon
        .route("/coupon/fetch", get(coupon::fetch).post(coupon::fetch))
        .route("/coupon/generate", post(coupon::generate))
        .route("/coupon/drop", post(coupon::drop))
        .route("/coupon/show", post(coupon::show))
        .route("/coupon/update", post(coupon::update))
        // Gift Card
        .route(
            "/gift-card/templates",
            get(gift_card::templates).post(gift_card::templates),
        )
        .route(
            "/gift-card/create-template",
            post(gift_card::create_template),
        )
        .route(
            "/gift-card/update-template",
            post(gift_card::update_template),
        )
        .route(
            "/gift-card/delete-template",
            post(gift_card::delete_template),
        )
        .route("/gift-card/generate-codes", post(gift_card::generate_codes))
        .route(
            "/gift-card/codes",
            get(gift_card::codes).post(gift_card::codes),
        )
        .route("/gift-card/toggle-code", post(gift_card::toggle_code))
        .route("/gift-card/delete-code", post(gift_card::delete_code))
        .route("/gift-card/export-codes", get(gift_card::export_codes))
        .route(
            "/gift-card/usages",
            get(gift_card::usages).post(gift_card::usages),
        )
        .route(
            "/gift-card/statistics",
            get(gift_card::statistics).post(gift_card::statistics),
        )
        .route("/gift-card/types", get(gift_card::types))
        // Knowledge
        .route("/knowledge/fetch", get(knowledge::fetch))
        .route("/knowledge/getCategory", get(knowledge::get_category))
        .route("/knowledge/save", post(knowledge::save))
        .route("/knowledge/show", post(knowledge::show))
        .route("/knowledge/drop", post(knowledge::drop))
        .route("/knowledge/sort", post(knowledge::sort))
        // Payment
        .route("/payment/fetch", get(payment::fetch))
        .route(
            "/payment/getPaymentMethods",
            get(payment::get_payment_methods),
        )
        .route("/payment/getPaymentForm", post(payment::get_payment_form))
        .route("/payment/save", post(payment::save))
        .route("/payment/show", post(payment::show))
        .route("/payment/drop", post(payment::drop))
        .route("/payment/sort", post(payment::sort))
        // System
        .route("/system/getSystemStatus", get(system::get_system_status))
        .route("/system/getQueueStats", get(system::get_queue_stats))
        .route("/system/getQueueWorkload", get(system::get_queue_workload))
        .route("/system/getQueueMasters", get(system::get_queue_masters))
        .route(
            "/system/getHorizonFailedJobs",
            get(system::get_horizon_failed_jobs),
        )
        .route(
            "/system/getAuditLog",
            get(system::get_audit_log).post(system::get_audit_log),
        )
        // Mail Template
        .route("/mail/template/list", get(mail_template::list))
        .route("/mail/template/get", get(mail_template::get))
        .route("/mail/template/save", post(mail_template::save))
        .route("/mail/template/reset", post(mail_template::reset))
        .route("/mail/template/test", post(mail_template::test))
        // Theme
        .route("/theme/getThemes", get(theme::get_themes))
        .route("/theme/getThemeConfig", post(theme::get_theme_config))
        .route("/theme/saveThemeConfig", post(theme::save_theme_config))
        .route("/theme/upload", post(theme::upload))
        .route("/theme/delete", post(theme::delete))
        // Plugin
        .route("/plugin/types", get(plugin::types))
        .route("/plugin/getPlugins", get(plugin::get_plugins))
        .route("/plugin/upload", post(plugin::success_action))
        .route("/plugin/delete", post(plugin::success_action))
        .route("/plugin/install", post(plugin::success_action))
        .route("/plugin/uninstall", post(plugin::success_action))
        .route("/plugin/enable", post(plugin::success_action))
        .route("/plugin/disable", post(plugin::success_action))
        .route("/plugin/upgrade", post(plugin::success_action))
        .route("/plugin/config", get(plugin::config).post(plugin::config))
        // Traffic Reset
        .route("/traffic-reset/logs", get(traffic_reset::logs))
        .route("/traffic-reset/stats", get(traffic_reset::stats))
        .route(
            "/traffic-reset/user/{userId}/history",
            get(traffic_reset::user_history),
        )
        .route("/traffic-reset/reset-user", post(traffic_reset::reset_user))
}
