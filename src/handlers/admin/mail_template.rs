use axum::{
    extract::Query,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct MailTemplateGetQuery {
    pub name: String,
}

/// GET /api/v2/admin/mail/template/list
pub async fn list(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    let templates = json!([
        { "name": "verify", "label": "邮箱验证码", "customized": false },
        { "name": "notify", "label": "站点通知", "customized": false },
        { "name": "remindExpire", "label": "到期提醒", "customized": false },
        { "name": "remindTraffic", "label": "流量提醒", "customized": false },
        { "name": "mailLogin", "label": "邮件登录", "customized": false }
    ]);
    Ok(ApiResponse::success(templates).into_response())
}

/// GET /api/v2/admin/mail/template/get
pub async fn get(
    _admin: AuthenticatedAdmin,
    Query(q): Query<MailTemplateGetQuery>,
) -> Result<Response, AppError> {
    let (label, req_vars, opt_vars, default_subj) = match q.name.as_str() {
        "verify" => (
            "邮箱验证码",
            vec!["code"],
            vec!["name", "url"],
            "邮箱验证码",
        ),
        "notify" => ("站点通知", vec!["content"], vec!["name", "url"], "站点通知"),
        "remindExpire" => ("到期提醒", vec![], vec!["name", "url"], "服务到期提醒"),
        "remindTraffic" => ("流量提醒", vec![], vec!["name", "url"], "流量不足提醒"),
        "mailLogin" => ("邮件登录", vec!["link"], vec!["name", "url"], "登录链接"),
        _ => return Err(AppError::NotFound("模板不存在".to_string())),
    };

    let data = json!({
        "name": q.name,
        "label": label,
        "required_vars": req_vars,
        "optional_vars": opt_vars,
        "customized": false,
        "subject": default_subj,
        "content": "<p>Content</p>"
    });

    Ok(ApiResponse::success(data).into_response())
}

/// POST /api/v2/admin/mail/template/save
pub async fn save(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!(true)).into_response())
}

/// POST /api/v2/admin/mail/template/reset
pub async fn reset(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!(true)).into_response())
}

/// POST /api/v2/admin/mail/template/test
pub async fn test(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!(true)).into_response())
}
