use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::{
    common::{AppError, AppState},
    handlers::AuthenticatedUser,
    services::UserService,
    utils::sha1_hex,
};

/// GET /api/v1/user/server/fetch
pub async fn fetch(
    State(state): State<AppState>,
    headers: HeaderMap,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Response, AppError> {
    let servers = if UserService::is_available(&user) {
        state
            .server_service
            .get_available_servers_for_user(&user)
            .await?
    } else {
        vec![]
    };

    let now = chrono::Utc::now().timestamp();
    let mut node_items = Vec::new();
    let mut cache_keys = Vec::new();

    for s in servers {
        let last_check_at = state.server_service.get_last_check_at(s.id).await;
        let is_online = last_check_at.map(|t| now - t < 300).unwrap_or(false);
        let cache_key = format!("{}-{}-{}-{}", s.r#type, s.id, s.updated_at, is_online);
        cache_keys.push(cache_key.clone());

        let tags_val = s
            .tags
            .as_deref()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok());

        node_items.push(json!({
            "id": s.id,
            "type": s.r#type,
            "version": serde_json::Value::Null,
            "name": s.name,
            "rate": s.rate,
            "tags": tags_val,
            "is_online": is_online,
            "cache_key": cache_key,
            "last_check_at": last_check_at,
        }));
    }

    let cache_keys_json = serde_json::to_string(&cache_keys).unwrap_or_default();
    let etag = sha1_hex(cache_keys_json.as_bytes());

    if let Some(if_none_match) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|h| h.to_str().ok())
    {
        if if_none_match.contains(&etag) {
            return Ok((StatusCode::NOT_MODIFIED, ()).into_response());
        }
    }

    let resp = json!({
        "data": node_items,
    });

    let res = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ETAG, format!("\"{}\"", etag))
        .body(axum::body::Body::from(
            serde_json::to_string(&resp).unwrap_or_default(),
        ))
        .unwrap_or_else(|_| (StatusCode::OK, Json(resp)).into_response());

    Ok(res)
}
