use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{user, User},
    protocols::{generate_subscription, ClientType, ProxyContext},
    services::UserService,
};

#[derive(Debug, serde::Deserialize, Default)]
pub struct AppQuery {
    pub token: Option<String>,
}

/// GET /api/v1/client/app/getVersion
pub async fn get_version(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_tidalab = ua.contains("tidalab/4.0.0") || ua.contains("tunnelab/4.0.0");

    let data = if is_tidalab {
        if ua.contains("Win64") {
            json!({
                "version": state.setting_service.get_string("windows_version", "").await,
                "download_url": state.setting_service.get_string("windows_download_url", "").await,
            })
        } else {
            json!({
                "version": state.setting_service.get_string("macos_version", "").await,
                "download_url": state.setting_service.get_string("macos_download_url", "").await,
            })
        }
    } else {
        json!({
            "windows_version": state.setting_service.get_string("windows_version", "").await,
            "windows_download_url": state.setting_service.get_string("windows_download_url", "").await,
            "macos_version": state.setting_service.get_string("macos_version", "").await,
            "macos_download_url": state.setting_service.get_string("macos_download_url", "").await,
            "android_version": state.setting_service.get_string("android_version", "").await,
            "android_download_url": state.setting_service.get_string("android_download_url", "").await,
        })
    };

    Ok(Json(ApiResponse::success(data)).into_response())
}

/// GET /api/v1/client/app/getConfig
pub async fn get_config(
    State(state): State<AppState>,
    Query(query): Query<AppQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let token = query
        .token
        .ok_or_else(|| AppError::Forbidden("token is null".into()))?;

    let user = User::find()
        .filter(user::Column::Token.eq(token))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Forbidden("token is error".into()))?;

    let servers = if UserService::is_available(&user) {
        state
            .server_service
            .get_available_servers_for_user(&user)
            .await?
    } else {
        vec![]
    };

    let app_name = state.setting_service.get_string("app_name", "Xboard").await;
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let context = ProxyContext {
        user_uuid: user.uuid,
        user_token: user.token,
        user_email: user.email,
        app_name,
        upload_bytes: user.u,
        download_bytes: user.d,
        total_bytes: user.transfer_enable,
        expired_at: user.expired_at.unwrap_or(0),
        host,
    };

    let output = generate_subscription(ClientType::Clash, &context, &servers);

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/x-yaml; charset=utf-8")],
        output.content,
    )
        .into_response())
}
