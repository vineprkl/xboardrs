use axum::{
    body::Body,
    http::{header, HeaderMap, Method, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::{common::AppState, utils::crc32b};

/// Determines the MIME content type from a file path extension.
pub fn mime_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "eot" => "application/vnd.ms-fontobject",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "webp" => "image/webp",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

/// Renders the User Dashboard SPA HTML with dynamic settings injected.
/// Matches PHP `ThemeService` and `theme::{theme}.dashboard`.
pub async fn render_user_dashboard(state: &AppState, headers: &HeaderMap) -> Response {
    // 1. Safe mode host validation
    let safe_mode_enable = state
        .setting_service
        .get_bool("safe_mode_enable", false)
        .await;
    if safe_mode_enable {
        let app_url = state.setting_service.get_string("app_url", "").await;
        if !app_url.is_empty() {
            let config_host = app_url
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("");

            let req_host = headers
                .get(header::HOST)
                .and_then(|h| h.to_str().ok())
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("");

            if !config_host.is_empty() && req_host != config_host {
                return (StatusCode::FORBIDDEN, "Forbidden").into_response();
            }
        }
    }

    // 2. Fetch configured parameters
    let title = state.setting_service.get_string("app_name", "Xboard").await;
    let theme = state
        .setting_service
        .get_string("frontend_theme", "Xboard")
        .await;
    let version = state
        .setting_service
        .get_string("app_version", "1.7.5")
        .await;
    let description = state
        .setting_service
        .get_string("app_description", "Xboard is best")
        .await;
    let logo = state.setting_service.get_string("logo", "").await;
    let theme_color = state
        .setting_service
        .get_string("frontend_theme_color", "default")
        .await;
    let background_url = state
        .setting_service
        .get_string("frontend_background_url", "")
        .await;
    let custom_html = state
        .setting_service
        .get_string("frontend_custom_html", "")
        .await;

    // Check if theme directory/assets exist on disk (Headless fallback for Form C)
    let base_dir = crate::utils::get_app_base_dir();
    let theme_candidates = [
        format!("theme/{}", theme),
        format!("../theme/{}", theme),
        format!("public/theme/{}", theme),
        format!("../public/theme/{}", theme),
        base_dir.join("theme").join(&theme).to_string_lossy().to_string(),
        base_dir.join("public/theme").join(&theme).to_string_lossy().to_string(),
    ];
    let theme_exists = theme_candidates.iter().any(|p| Path::new(p).exists());

    if !theme_exists {
        let payload = serde_json::json!({
            "code": 200,
            "data": {
                "app_name": title,
                "version": version,
                "status": "online",
                "mode": "headless_api",
                "message": "Xboard-RS API Gateway is running in headless mode. Frontend theme is not installed.",
                "endpoints": {
                    "user_api": "/api/v1/user/*",
                    "passport_api": "/api/v1/passport/*",
                    "guest_api": "/api/v1/guest/*",
                    "admin_api": "/api/v2/admin/*",
                    "subscribe": "/api/v1/client/subscribe"
                }
            }
        });
        return axum::Json(payload).into_response();
    }

    let html = format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1,minimum-scale=1,user-scalable=no" />
  <title>{title}</title>
  <script type="module" crossorigin src="/theme/{theme}/assets/umi.js"></script>
</head>
<body>
  <script>
    window.routerBase = "/";
    window.settings = {{
      title: '{title}',
      assets_path: '/theme/{theme}/assets',
      theme: {{
        color: '{theme_color}',
      }},
      version: '{version}',
      background_url: '{background_url}',
      description: '{description}',
      i18n: [
        'zh-CN',
        'en-US',
        'ja-JP',
        'vi-VN',
        'ko-KR',
        'zh-TW',
        'fa-IR'
      ],
      logo: '{logo}'
    }}
  </script>
  <div id="app"></div>
  {custom_html}
</body>
</html>"#
    );

    Html(html).into_response()
}

/// Renders the React Admin SPA HTML with dynamic settings injected.
/// Matches PHP `admin.blade.php`.
pub async fn render_admin_dashboard(state: &AppState) -> Response {
    let title = state.setting_service.get_string("app_name", "XBoard").await;
    let version = state
        .setting_service
        .get_string("app_version", "1.7.5")
        .await;
    let logo = state.setting_service.get_string("logo", "").await;

    let mut secure_path = state.setting_service.get_string("secure_path", "").await;
    if secure_path.is_empty() {
        secure_path = state
            .setting_service
            .get_string("frontend_admin_path", "")
            .await;
    }
    if secure_path.is_empty() {
        let app_key = std::env::var("APP_KEY")
            .unwrap_or_else(|_| "base64:xboard_default_key_32bytes!!".to_string());
        secure_path = crc32b(app_key.as_bytes());
    }

    // Check if manifest.json exists in candidate paths
    let base_dir = crate::utils::get_app_base_dir();
    let manifest_candidates = [
        "public/assets/admin/manifest.json".to_string(),
        "../public/assets/admin/manifest.json".to_string(),
        "assets/admin/manifest.json".to_string(),
        "../assets/admin/manifest.json".to_string(),
        base_dir.join("public/assets/admin/manifest.json").to_string_lossy().to_string(),
        base_dir.join("assets/admin/manifest.json").to_string_lossy().to_string(),
    ];
    let manifest_path = manifest_candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file());

    let admin_assets_exist = manifest_path.is_some()
        || [
            "public/assets/admin".to_string(),
            "../public/assets/admin".to_string(),
            "assets/admin".to_string(),
            "../assets/admin".to_string(),
            base_dir.join("public/assets/admin").to_string_lossy().to_string(),
            base_dir.join("assets/admin").to_string_lossy().to_string(),
        ]
        .iter()
        .any(|p| Path::new(p).exists());

    if !admin_assets_exist {
        let payload = serde_json::json!({
            "code": 200,
            "data": {
                "app_name": title,
                "version": version,
                "status": "online",
                "mode": "headless_admin",
                "message": "Xboard-RS Admin API Gateway is active. Admin frontend assets are not installed.",
                "secure_path": secure_path
            }
        });
        return axum::Json(payload).into_response();
    }

    let mut styles_and_scripts = String::new();

    if let Some(path) = manifest_path {
        if let Ok(content) = fs::read_to_string(&path).await {
            if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(entry) = manifest.get("index.html") {
                    if let Some(css_array) = entry.get("css").and_then(|c| c.as_array()) {
                        for c in css_array {
                            if let Some(file) = c.as_str() {
                                styles_and_scripts.push_str(&format!(
                                    r#"  <link rel="stylesheet" crossorigin href="/assets/admin/{file}" />{}"#,
                                    "\n"
                                ));
                            }
                        }
                    }

                    // Dynamically discover locales in public/assets/admin/locales/*.js
                    let locale_candidates = [
                        "public/assets/admin/locales".to_string(),
                        "../public/assets/admin/locales".to_string(),
                        "assets/admin/locales".to_string(),
                        "../assets/admin/locales".to_string(),
                        base_dir.join("public/assets/admin/locales").to_string_lossy().to_string(),
                        base_dir.join("assets/admin/locales").to_string_lossy().to_string(),
                    ];
                    let mut locales = Vec::new();
                    for dir in &locale_candidates {
                        if let Ok(mut entries) = fs::read_dir(dir).await {
                            while let Ok(Some(entry)) = entries.next_entry().await {
                                let p = entry.path();
                                if p.extension().and_then(|e| e.to_str()) == Some("js") {
                                    if let Some(file_name) = p.file_name().and_then(|n| n.to_str())
                                    {
                                        locales.push(format!("locales/{}", file_name));
                                    }
                                }
                            }
                            if !locales.is_empty() {
                                locales.sort();
                                break;
                            }
                        }
                    }
                    for loc in locales {
                        styles_and_scripts.push_str(&format!(
                            r#"  <script src="/assets/admin/{loc}"></script>{}"#,
                            "\n"
                        ));
                    }

                    if let Some(js_file) = entry.get("file").and_then(|f| f.as_str()) {
                        styles_and_scripts.push_str(&format!(
                            r#"  <script type="module" crossorigin src="/assets/admin/{js_file}"></script>{}"#,
                            "\n"
                        ));
                    }
                }
            }
        }
    }

    // Fallback if manifest didn't populate styles/scripts
    if styles_and_scripts.is_empty() {
        styles_and_scripts =
            r#"  <link rel="stylesheet" crossorigin href="/assets/admin/assets/index.css" />
  <link rel="stylesheet" crossorigin href="/assets/admin/assets/vendor.css" />
  <script src="/assets/admin/locales/en-US.js"></script>
  <script src="/assets/admin/locales/zh-CN.js"></script>
  <script src="/assets/admin/locales/ko-KR.js"></script>
  <script type="module" crossorigin src="/assets/admin/assets/index.js"></script>"#
                .to_string();
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>{title}</title>
  <script>
    window.settings = {{
      base_url: "/",
      title: "{title}",
      version: "{version}",
      logo: "{logo}",
      secure_path: "{secure_path}",
    }};
  </script>
{styles_and_scripts}
</head>
<body>
  <div id="root"></div>
</body>
</html>"#
    );

    Html(html).into_response()
}

/// Attempts to serve a static file from a list of candidate base directories.
pub async fn try_serve_static_file(rel_path: &str, candidates: &[&str]) -> Option<Response> {
    let clean_rel = rel_path.trim_start_matches('/').replace('\\', "/");
    // Path traversal protection
    if clean_rel.contains("..") {
        return None;
    }

    for base in candidates {
        let full_path = PathBuf::from(base).join(&clean_rel);
        if full_path.is_file() {
            if let Ok(bytes) = fs::read(&full_path).await {
                let mime = mime_for_path(&full_path);
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, mime)
                    .body(Body::from(bytes))
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
                return Some(response);
            }
        }
    }

    None
}

/// Dynamic web router and SPA fallback for both User and Admin frontends.
pub async fn web_fallback_handler(
    state: &AppState,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
) -> Response {
    let path = uri.path();
    let trimmed = path.trim_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();

    // 1. Static file requests: /theme/*, /assets/*, /favicon.ico, /robots.txt
    let base_dir = crate::utils::get_app_base_dir();
    let base_theme = base_dir.join("theme").to_string_lossy().to_string();
    let base_pub_theme = base_dir.join("public/theme").to_string_lossy().to_string();
    let base_pub_assets = base_dir.join("public/assets").to_string_lossy().to_string();
    let base_assets = base_dir.join("assets").to_string_lossy().to_string();
    let base_pub = base_dir.join("public").to_string_lossy().to_string();

    if let Some(sub) = path.strip_prefix("/theme") {
        let mut candidates = vec![
            "theme",
            "../theme",
            "public/theme",
            "../public/theme",
            &base_theme,
            &base_pub_theme,
        ];
        let env_theme = std::env::var("THEME_PATH").unwrap_or_default();
        if !env_theme.is_empty() {
            candidates.insert(0, &env_theme);
        }
        if let Some(resp) = try_serve_static_file(sub, &candidates).await {
            return resp;
        }
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Some(sub) = path.strip_prefix("/assets") {
        let mut candidates = vec![
            "public/assets",
            "../public/assets",
            "assets",
            "../assets",
            &base_pub_assets,
            &base_assets,
        ];
        let env_public = std::env::var("PUBLIC_PATH").unwrap_or_default();
        let env_assets = if !env_public.is_empty() {
            format!("{}/assets", env_public)
        } else {
            String::new()
        };
        if !env_assets.is_empty() {
            candidates.insert(0, &env_assets);
        }
        if let Some(resp) = try_serve_static_file(sub, &candidates).await {
            return resp;
        }
        return StatusCode::NOT_FOUND.into_response();
    }

    if path == "/favicon.ico" || path == "/robots.txt" {
        let candidates = ["public", "../public", ".", "..", &base_pub];
        if let Some(resp) = try_serve_static_file(path, &candidates).await {
            return resp;
        }
        if path == "/robots.txt" {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Body::from("User-agent: *\nDisallow: /api/\n"))
                .unwrap();
        }
        return StatusCode::NOT_FOUND.into_response();
    }

    // Direct static file under public (e.g. /favicon.png, /logo.png, etc.)
    if path.contains('.') && !path.ends_with(".php") && !path.contains(".env") {
        let candidates = ["public", "../public", &base_pub];
        if let Some(resp) = try_serve_static_file(path, &candidates).await {
            return resp;
        }
    }

    // Only GET / HEAD requests are candidate for SPA fallback
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::NOT_FOUND.into_response();
    }

    // 2. Admin frontend SPA: /{secure_path} or /{secure_path}/*
    let secure_path = state.setting_service.get_string("secure_path", "").await;
    let frontend_admin_path = state
        .setting_service
        .get_string("frontend_admin_path", "")
        .await;
    let app_key = std::env::var("APP_KEY")
        .unwrap_or_else(|_| "base64:xboard_default_key_32bytes!!".to_string());
    let fallback_crc = crc32b(app_key.as_bytes());

    let is_admin_spa = if !parts.is_empty() && !parts[0].is_empty() {
        (!secure_path.is_empty() && parts[0] == secure_path)
            || (!frontend_admin_path.is_empty() && parts[0] == frontend_admin_path)
            || parts[0] == fallback_crc
    } else {
        false
    };

    if is_admin_spa {
        return render_admin_dashboard(state).await;
    }

    // 3. User frontend SPA: root or standard frontend pages
    let is_user_spa = if path == "/" || path.is_empty() || path == "/index.html" {
        true
    } else if parts.len() == 1 || (parts.len() == 2 && parts[0] == "order") {
        matches!(
            parts[0],
            "login"
                | "register"
                | "dashboard"
                | "plan"
                | "order"
                | "ticket"
                | "knowledge"
                | "profile"
                | "forget"
                | "reset"
                | "traffic"
        )
    } else {
        false
    };

    if is_user_spa {
        return render_user_dashboard(state, headers).await;
    }

    StatusCode::NOT_FOUND.into_response()
}
