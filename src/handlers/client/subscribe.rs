use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;

use crate::{
    common::{AppError, AppState},
    entities::{user, User},
    protocols::{detect_client, generate_subscription, ProxyContext},
    services::UserService,
    utils::traffic_format,
};

#[derive(Debug, Deserialize, Default)]
pub struct SubscribeQuery {
    pub token: Option<String>,
    pub types: Option<String>,
    pub filter: Option<String>,
    pub flag: Option<String>,
}

/// Helper to get protocol prefix for server names
fn get_protocol_prefix(server_type: &str, protocol_settings: Option<&str>) -> &'static str {
    match server_type {
        "vless" => "[vless]",
        "shadowsocks" => "[ss]",
        "vmess" => "[vmess]",
        "trojan" => "[trojan]",
        "tuic" => "[tuic]",
        "socks" => "[socks]",
        "anytls" => "[anytls]",
        "hysteria" => {
            if let Some(ps) = protocol_settings {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(ps) {
                    if v.get("version").and_then(|ver| ver.as_i64()) == Some(1) {
                        return "[Hy]";
                    }
                }
            }
            "[Hy2]"
        }
        _ => "",
    }
}

/// Core subscription processor handling token validation, server filtering, and protocol output.
pub async fn handle_subscribe(
    state: &AppState,
    token: &str,
    query: &SubscribeQuery,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    if token.trim().is_empty() {
        return Err(AppError::Forbidden("token is null".into()));
    }

    // 1. Fetch user by token
    let user = User::find()
        .filter(user::Column::Token.eq(token))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Forbidden("token is error".into()))?;

    // 2. Check user availability (not banned, quota > 0, not expired)
    if !UserService::is_available(&user) {
        return Ok((
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "User is unavailable",
        )
            .into_response());
    }

    // 3. Fetch available servers for user's group
    let mut servers = state
        .server_service
        .get_available_servers_for_user(&user)
        .await?;

    let original_count = servers.len();

    // 4. Filter by types parameter if specified (e.g. "vless,trojan" or "vmess|shadowsocks")
    if let Some(ref types_str) = query.types {
        let allowed_types: Vec<String> = types_str
            .split(&[',', '|', '｜'][..])
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        if !allowed_types.is_empty() && allowed_types[0] != "all" {
            servers.retain(|s| allowed_types.contains(&s.r#type.to_lowercase()));
        }
    }

    // 5. Filter by filter keyword parameter if specified
    if let Some(ref filter_str) = query.filter {
        let keywords: Vec<String> = filter_str
            .split(&[',', '|', '｜'][..])
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        if !keywords.is_empty() {
            servers.retain(|s| {
                let name_lower = s.name.to_lowercase();
                let tags_lower = s.tags.as_deref().unwrap_or("").to_lowercase();
                keywords
                    .iter()
                    .any(|kw| name_lower.contains(kw) || tags_lower.contains(kw))
            });
        }
    }

    let rejected_count = original_count.saturating_sub(servers.len());

    // 6. Prepend informational status nodes (matching PHP setSubscribeInfoToServers)
    if !servers.is_empty() {
        let base_tpl = servers[0].clone();
        let mut info_nodes = Vec::new();

        // If rejected servers, add notice
        if rejected_count > 0 {
            let mut n_reject = base_tpl.clone();
            n_reject.name = format!("过滤掉{}条线路", rejected_count);
            info_nodes.push(n_reject);
        }

        let show_info = state
            .setting_service
            .get_bool("show_info_to_server_enable", false)
            .await;

        if show_info {
            let remaining = user.transfer_enable.saturating_sub(user.u + user.d);
            let remaining_str = traffic_format(remaining);
            let expire_str = match user.expired_at {
                Some(ts) => chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "长期有效".to_string()),
                None => "长期有效".to_string(),
            };

            let mut n_expire = base_tpl.clone();
            n_expire.name = format!("套餐到期：{}", expire_str);

            let n_reset = if let Some(ts) = user.next_reset_at {
                let now = chrono::Utc::now().timestamp();
                if ts > now {
                    let days = ((ts - now) as f64 / 86400.0).ceil() as i64;
                    let mut n = base_tpl.clone();
                    n.name = format!("距离下次重置剩余：{} 天", days);
                    Some(n)
                } else {
                    None
                }
            } else {
                None
            };

            let mut n_traffic = base_tpl;
            n_traffic.name = format!("剩余流量：{}", remaining_str);

            let mut top = Vec::new();
            top.push(n_traffic);
            if let Some(r) = n_reset {
                top.push(r);
            }
            top.push(n_expire);

            // In PHP array_unshift, earlier unshifts are pushed down:
            // Final order: traffic, reset, expire, reject, servers
            top.append(&mut info_nodes);
            info_nodes = top;
        }

        info_nodes.append(&mut servers);
        servers = info_nodes;
    }

    // 7. Prepend protocol prefix if show_protocol_to_server_enable is true
    let show_protocol = state
        .setting_service
        .get_bool("show_protocol_to_server_enable", false)
        .await;
    if show_protocol {
        for s in servers.iter_mut() {
            let prefix = get_protocol_prefix(&s.r#type, s.protocol_settings.as_deref());
            if !prefix.is_empty() {
                s.name = format!("{}{}", prefix, s.name);
            }
        }
    }

    // 8. Build ProxyContext
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let client_type = detect_client(query.flag.as_deref(), user_agent);
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

    // 9. Generate Subscription Content via Phase 3 Engine
    let output = generate_subscription(client_type, &context, &servers);

    // 10. Assemble HTTP Response
    let user_info_header = context.user_info_header();
    let content_disposition = format!("attachment; filename=\"{}\"", output.filename);

    Ok((
        StatusCode::OK,
        [
            ("subscription-userinfo", user_info_header.as_str()),
            (header::CONTENT_TYPE.as_str(), output.content_type),
            (
                header::CONTENT_DISPOSITION.as_str(),
                content_disposition.as_str(),
            ),
        ],
        output.content,
    )
        .into_response())
}

/// GET /{subscribe_path}/{token} (e.g. /s/{token})
pub async fn subscribe_by_path(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Query<SubscribeQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    handle_subscribe(&state, &token, &query, &headers).await
}

/// GET /api/v1/client/subscribe?token={token}
pub async fn subscribe_legacy(
    State(state): State<AppState>,
    Query(query): Query<SubscribeQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let token = query
        .token
        .clone()
        .ok_or_else(|| AppError::Forbidden("token is null".into()))?;

    handle_subscribe(&state, &token, &query, &headers).await
}
