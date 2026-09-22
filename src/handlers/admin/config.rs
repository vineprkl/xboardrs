use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{setting, Setting},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct ConfigFetchQuery {
    pub key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramWebhookRequest {
    pub telegram_bot_token: Option<String>,
}

/// GET /api/v2/admin/config/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<ConfigFetchQuery>,
) -> Result<Response, AppError> {
    let s = &state.setting_service;

    let invite_map = json!({
        "invite_force": s.get_bool("invite_force", false).await,
        "invite_commission": s.get_float("invite_commission", 10.0).await,
        "invite_gen_limit": s.get_int("invite_gen_limit", 5).await,
        "invite_never_expire": s.get_bool("invite_never_expire", false).await,
        "commission_first_time_enable": s.get_bool("commission_first_time_enable", true).await,
        "commission_auto_check_enable": s.get_bool("commission_auto_check_enable", true).await,
        "commission_withdraw_limit": s.get_int("commission_withdraw_limit", 100).await,
        "commission_withdraw_method": s.get_optional_string("commission_withdraw_method").await,
        "withdraw_close_enable": s.get_bool("withdraw_close_enable", false).await,
        "commission_distribution_enable": s.get_bool("commission_distribution_enable", false).await,
        "commission_distribution_l1": s.get_float("commission_distribution_l1", 0.0).await,
        "commission_distribution_l2": s.get_float("commission_distribution_l2", 0.0).await,
        "commission_distribution_l3": s.get_float("commission_distribution_l3", 0.0).await,
    });

    let site_map = json!({
        "stop_register": s.get_bool("stop_register", false).await,
        "email_verify": s.get_bool("email_verify", false).await,
        "app_name": s.get_string("app_name", "XBoard").await,
        "app_description": s.get_string("app_description", "Xboard is best").await,
        "app_url": s.get_string("app_url", "").await,
        "subscribe_url": s.get_string("subscribe_url", "").await,
        "try_out_plan_id": s.get_int("try_out_plan_id", 0).await,
        "try_out_hour": s.get_int("try_out_hour", 1).await,
        "tos_url": s.get_string("tos_url", "").await,
        "currency": s.get_string("currency", "CNY").await,
        "currency_symbol": s.get_string("currency_symbol", "¥").await,
    });

    let subscribe_map = json!({
        "plan_change_enable": s.get_bool("plan_change_enable", true).await,
        "reset_traffic_method": s.get_int("reset_traffic_method", 0).await,
        "surplus_enable": s.get_bool("surplus_enable", true).await,
        "new_order_event_id": s.get_int("new_order_event_id", 0).await,
        "renew_order_event_id": s.get_int("renew_order_event_id", 0).await,
        "change_order_event_id": s.get_int("change_order_event_id", 0).await,
        "show_info_to_server_enable": s.get_bool("show_info_to_server_enable", false).await,
        "show_protocol_to_server_enable": s.get_bool("show_protocol_to_server_enable", false).await,
    });

    let frontend_map = json!({
        "frontend_theme": s.get_string("frontend_theme", "Xboard").await,
        "frontend_theme_sidebar": s.get_string("frontend_theme_sidebar", "light").await,
        "frontend_theme_header": s.get_string("frontend_theme_header", "dark").await,
        "frontend_theme_color": s.get_string("frontend_theme_color", "default").await,
        "frontend_background_url": s.get_string("frontend_background_url", "").await,
        "frontend_admin_path": s.get_string("frontend_admin_path", "admin").await,
        "secure_path": s.get_string("secure_path", "admin").await,
    });

    let server_map = json!({
        "server_token": s.get_string("server_token", "").await,
        "server_pull_interval": s.get_int("server_pull_interval", 60).await,
        "server_push_interval": s.get_int("server_push_interval", 60).await,
    });

    let email_map = json!({
        "email_host": s.get_string("email_host", "").await,
        "email_port": s.get_int("email_port", 465).await,
        "email_username": s.get_string("email_username", "").await,
        "email_encryption": s.get_string("email_encryption", "ssl").await,
        "email_from_address": s.get_string("email_from_address", "").await,
        "email_whitelist_enable": s.get_bool("email_whitelist_enable", false).await,
        "email_whitelist_suffix": s.get_optional_string("email_whitelist_suffix").await,
        "email_gmail_limit_enable": s.get_bool("email_gmail_limit_enable", false).await,
    });

    let telegram_map = json!({
        "telegram_bot_enable": s.get_bool("telegram_bot_enable", false).await,
        "telegram_bot_token": s.get_string("telegram_bot_token", "").await,
    });

    let app_map = json!({
        "windows_version": s.get_string("windows_version", "").await,
        "windows_download_url": s.get_string("windows_download_url", "").await,
        "macos_version": s.get_string("macos_version", "").await,
        "macos_download_url": s.get_string("macos_download_url", "").await,
        "android_version": s.get_string("android_version", "").await,
        "android_download_url": s.get_string("android_download_url", "").await,
    });

    let safe_map = json!({
        "email_verify_code_ttl": s.get_int("email_verify_code_ttl", 300).await,
        "password_limit_enable": s.get_bool("password_limit_enable", true).await,
        "password_limit_count": s.get_int("password_limit_count", 5).await,
    });

    let currency_map = json!({
        "currency": s.get_string("currency", "CNY").await,
        "currency_symbol": s.get_string("currency_symbol", "¥").await,
    });

    let full_map = json!({
        "invite": invite_map,
        "site": site_map,
        "subscribe": subscribe_map,
        "frontend": frontend_map,
        "server": server_map,
        "email": email_map,
        "telegram": telegram_map,
        "app": app_map,
        "safe": safe_map,
        "currency": currency_map,
    });

    if let Some(ref key) = query.key {
        if let Some(val) = full_map.get(key) {
            return Ok(ApiResponse::success(json!({ key: val })).into_response());
        }
    }

    Ok(ApiResponse::success(full_map).into_response())
}

/// POST /api/v2/admin/config/save
pub async fn save(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<HashMap<String, Value>>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();

    for (k, v) in payload {
        let val_str = match v {
            Value::String(s) => s,
            Value::Null => "".to_string(),
            other => other.to_string(),
        };

        let existing = Setting::find()
            .filter(setting::Column::Name.eq(&k))
            .one(&state.db)
            .await?;

        if let Some(model) = existing {
            let mut active: setting::ActiveModel = model.into();
            active.value = Set(Some(val_str));
            active.updated_at = Set(Some(now));
            active.update(&state.db).await?;
        } else {
            let new_s = setting::ActiveModel {
                name: Set(k),
                value: Set(Some(val_str)),
                created_at: Set(Some(now)),
                updated_at: Set(Some(now)),
                ..Default::default()
            };
            new_s.insert(&state.db).await?;
        }
    }

    // Refresh memory cache in SettingService
    state.setting_service.load_all().await?;

    Ok(ApiResponse::success(true).into_response())
}

/// GET /api/v2/admin/config/getEmailTemplate
pub async fn get_email_template(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    let templates = vec!["notify", "remindMail", "verifyMail"];
    Ok(ApiResponse::success(templates).into_response())
}

/// GET /api/v2/admin/config/getThemeTemplate
pub async fn get_theme_template(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    let themes = vec!["Xboard"];
    Ok(ApiResponse::success(themes).into_response())
}

/// POST /api/v2/admin/config/setTelegramWebhook
pub async fn set_telegram_webhook(
    _admin: AuthenticatedAdmin,
    Json(payload): Json<TelegramWebhookRequest>,
) -> Result<Response, AppError> {
    let res = json!({
        "success": true,
        "webhook_url": "https://api.telegram.org",
        "token": payload.telegram_bot_token.unwrap_or_default()
    });
    Ok(ApiResponse::success(res).into_response())
}

/// POST /api/v2/admin/config/testSendMail
pub async fn test_send_mail(admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    let res = json!({
        "success": true,
        "recipient": admin.0.email,
        "message": "Test email simulated successfully"
    });
    Ok(ApiResponse::success(res).into_response())
}
