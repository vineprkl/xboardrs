use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError, AppState},
    handlers::auth::AuthenticatedAdmin,
};

#[derive(Debug, Deserialize)]
pub struct ThemeNamePayload {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveThemeConfigPayload {
    pub name: String,
    pub config: serde_json::Value,
}

/// GET /api/v2/admin/theme/getThemes
pub async fn get_themes(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let active = state
        .setting_service
        .get_string("frontend_theme", "Xboard")
        .await;

    let mut themes = serde_json::Map::new();
    let theme_config_path = std::path::Path::new("theme/Xboard/config.json");
    let alt_path = std::path::Path::new("../theme/Xboard/config.json");

    let chosen_path = if theme_config_path.is_file() {
        Some(theme_config_path)
    } else if alt_path.is_file() {
        Some(alt_path)
    } else {
        None
    };

    let mut xboard_config = if let Some(p) = chosen_path {
        tokio::fs::read_to_string(p)
            .await
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .unwrap_or_else(|| json!({ "name": "Xboard" }))
    } else {
        json!({ "name": "Xboard" })
    };

    if let Some(obj) = xboard_config.as_object_mut() {
        obj.insert("can_delete".to_string(), json!(false));
        obj.insert("is_system".to_string(), json!(true));
    }
    themes.insert("Xboard".to_string(), xboard_config);

    let data = json!({
        "themes": themes,
        "active": active,
    });
    Ok(ApiResponse::success(data).into_response())
}

/// POST /api/v2/admin/theme/getThemeConfig
pub async fn get_theme_config(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<ThemeNamePayload>,
) -> Result<Response, AppError> {
    let key = format!("theme_{}", payload.name);
    let config_str = state.setting_service.get_string(&key, "{}").await;
    let val: serde_json::Value = serde_json::from_str(&config_str).unwrap_or_else(|_| json!({}));
    Ok(ApiResponse::success(val).into_response())
}

/// POST /api/v2/admin/theme/saveThemeConfig
pub async fn save_theme_config(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<SaveThemeConfigPayload>,
) -> Result<Response, AppError> {
    let key = format!("theme_{}", payload.name);
    let val_str = serde_json::to_string(&payload.config).unwrap_or_else(|_| "{}".to_string());
    state.setting_service.set(&key, &val_str).await?;
    Ok(ApiResponse::success(payload.config).into_response())
}

/// POST /api/v2/admin/theme/upload
pub async fn upload(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!(true)).into_response())
}

/// POST /api/v2/admin/theme/delete
pub async fn delete(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!(true)).into_response())
}
