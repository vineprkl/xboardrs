use std::collections::BTreeMap;

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use regex::Regex;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{knowledge, user, Knowledge},
    handlers::AuthenticatedUser,
    services::UserService,
    utils::get_subscribe_url,
};

#[derive(Debug, Deserialize, Default)]
pub struct KnowledgeQuery {
    pub id: Option<i32>,
    pub language: Option<String>,
    pub keyword: Option<String>,
}

fn process_knowledge_body(
    body: &str,
    user: &user::Model,
    is_user_available: bool,
    app_name: &str,
    subscribe_url: &str,
) -> String {
    let mut processed = body.to_string();

    // Mask restricted section if user is not available
    if !is_user_available {
        if let Ok(re) = Regex::new(r"(?s)<!--access start-->(.*?)<!--access end-->") {
            processed = re
                .replace_all(
                    &processed,
                    "<div class=\"v2board-no-access\">You must have a valid subscription to view content in this area</div>",
                )
                .to_string();
        }
    }

    // Replace template variables
    processed = processed.replace("{{siteName}}", app_name);
    processed = processed.replace("{{subscribeUrl}}", subscribe_url);
    processed = processed.replace(
        "{{urlEncodeSubscribeUrl}}",
        &urlencoding::encode(subscribe_url),
    );

    let safe_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        subscribe_url.as_bytes(),
    )
    .replace('+', "-")
    .replace('/', "_")
    .replace('=', "");
    processed = processed.replace("{{safeBase64SubscribeUrl}}", &safe_b64);

    let _ = user;
    processed
}

/// GET /api/v1/user/knowledge/fetch
pub async fn fetch(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Response, AppError> {
    let is_avail = UserService::is_available(&user);
    let app_name = state.setting_service.get_string("app_name", "XBoard").await;
    let base_url = state.setting_service.get_string("app_url", "").await;
    let sub_path = state
        .setting_service
        .get_string("subscribe_path", "s")
        .await;
    let sub_url = get_subscribe_url(&base_url, &sub_path, &user.token);

    if let Some(id) = query.id {
        let item = Knowledge::find_by_id(id)
            .filter(knowledge::Column::Show.eq(true))
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(500, "Article does not exist".into()))?;

        let body = process_knowledge_body(&item.body, &user, is_avail, &app_name, &sub_url);
        let resp = json!({
            "id": item.id,
            "category": item.category,
            "title": item.title,
            "body": body,
            "updated_at": item.updated_at,
        });

        return Ok(Json(ApiResponse::success(resp)).into_response());
    }

    let mut q = Knowledge::find()
        .filter(knowledge::Column::Show.eq(true))
        .order_by_asc(knowledge::Column::Sort)
        .order_by_asc(knowledge::Column::Id);

    if let Some(lang) = query.language {
        if !lang.trim().is_empty() {
            q = q.filter(knowledge::Column::Language.eq(lang.trim()));
        }
    }

    if let Some(kw) = query.keyword {
        let kw = kw.trim();
        if !kw.is_empty() {
            q = q.filter(
                knowledge::Column::Title
                    .contains(kw)
                    .or(knowledge::Column::Body.contains(kw)),
            );
        }
    }

    let items = q.all(&state.db).await?;

    let mut grouped: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    for item in items {
        let body = process_knowledge_body(&item.body, &user, is_avail, &app_name, &sub_url);
        let entry = json!({
            "id": item.id,
            "category": item.category.clone(),
            "title": item.title,
            "body": body,
            "updated_at": item.updated_at,
        });
        grouped.entry(item.category).or_default().push(entry);
    }

    Ok(Json(ApiResponse::success(grouped)).into_response())
}

/// GET /api/v1/user/knowledge/getCategory
pub async fn get_category(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Response, AppError> {
    let mut q = Knowledge::find()
        .filter(knowledge::Column::Show.eq(true))
        .select_only()
        .column(knowledge::Column::Category)
        .distinct();

    if let Some(lang) = query.language {
        if !lang.trim().is_empty() {
            q = q.filter(knowledge::Column::Language.eq(lang.trim()));
        }
    }

    let items: Vec<String> = q.into_tuple().all(&state.db).await?;

    Ok(Json(ApiResponse::success(items)).into_response())
}
