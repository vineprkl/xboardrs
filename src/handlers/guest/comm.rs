use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::common::{ApiResponse, AppError, AppState};

const DEFAULT_EMAIL_WHITELIST_SUFFIX: &[&str] = &[
    "gmail.com",
    "qq.com",
    "163.com",
    "yahoo.com",
    "sina.com",
    "126.com",
    "outlook.com",
    "yeah.net",
    "foxmail.com",
];

/// GET /api/v1/guest/comm/config
pub async fn config(State(state): State<AppState>) -> Result<Response, AppError> {
    let tos_url = state.setting_service.get_optional_string("tos_url").await;
    let is_email_verify = if state.setting_service.get_bool("email_verify", false).await {
        1
    } else {
        0
    };
    let is_invite_force = if state.setting_service.get_bool("invite_force", false).await {
        1
    } else {
        0
    };

    let email_whitelist_enable = state
        .setting_service
        .get_bool("email_whitelist_enable", false)
        .await;
    let email_whitelist_suffix = if email_whitelist_enable {
        let suffix_setting = state
            .setting_service
            .get_string("email_whitelist_suffix", "")
            .await;
        if suffix_setting.trim().is_empty() {
            json!(DEFAULT_EMAIL_WHITELIST_SUFFIX)
        } else if let Ok(arr) = serde_json::from_str::<serde_json::Value>(&suffix_setting) {
            arr
        } else {
            let list: Vec<&str> = suffix_setting
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            json!(list)
        }
    } else {
        json!(0)
    };

    let is_captcha = if state
        .setting_service
        .get_bool("captcha_enable", false)
        .await
    {
        1
    } else {
        0
    };
    let captcha_type = state
        .setting_service
        .get_string("captcha_type", "recaptcha")
        .await;
    let recaptcha_site_key = state
        .setting_service
        .get_optional_string("recaptcha_site_key")
        .await;
    let recaptcha_v3_site_key = state
        .setting_service
        .get_optional_string("recaptcha_v3_site_key")
        .await;
    let recaptcha_v3_score_threshold = state
        .setting_service
        .get_json::<serde_json::Value>("recaptcha_v3_score_threshold")
        .await
        .unwrap_or(json!(0.5));
    let turnstile_site_key = state
        .setting_service
        .get_optional_string("turnstile_site_key")
        .await;
    let app_description = state
        .setting_service
        .get_optional_string("app_description")
        .await;
    let app_url = state.setting_service.get_optional_string("app_url").await;
    let logo = state.setting_service.get_optional_string("logo").await;

    let data = json!({
        "tos_url": tos_url,
        "is_email_verify": is_email_verify,
        "is_invite_force": is_invite_force,
        "email_whitelist_suffix": email_whitelist_suffix,
        "is_captcha": is_captcha,
        "captcha_type": captcha_type,
        "recaptcha_site_key": recaptcha_site_key,
        "recaptcha_v3_site_key": recaptcha_v3_site_key,
        "recaptcha_v3_score_threshold": recaptcha_v3_score_threshold,
        "turnstile_site_key": turnstile_site_key,
        "app_description": app_description,
        "app_url": app_url,
        "logo": logo,
        "is_recaptcha": is_captcha,
    });

    Ok(Json(ApiResponse::success(data)).into_response())
}
