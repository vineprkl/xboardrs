use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError},
    handlers::auth::AuthenticatedAdmin,
};

/// GET /api/v2/admin/plugin/types
pub async fn types(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    let data = json!([
        {
            "value": "feature",
            "label": "功能",
            "description": "提供功能扩展的插件，如Telegram登录、邮件通知等",
            "icon": "🔧"
        },
        {
            "value": "payment",
            "label": "支付方式",
            "description": "提供支付接口的插件，如支付宝、微信支付等",
            "icon": "💳"
        }
    ]);
    Ok(ApiResponse::success(data).into_response())
}

/// GET /api/v2/admin/plugin/getPlugins
pub async fn get_plugins(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!([])).into_response())
}

/// POST /api/v2/admin/plugin/upload, install, uninstall, enable, disable, upgrade, delete
pub async fn success_action(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!(true)).into_response())
}

/// GET /api/v2/admin/plugin/config or POST
pub async fn config(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!({})).into_response())
}
