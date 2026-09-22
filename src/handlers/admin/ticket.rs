use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{AdminTableQuery, ApiResponse, AppError, AppState, PaginatedResponse},
    entities::{ticket, ticket_message, Ticket, TicketMessage, User},
    handlers::auth::AuthenticatedAdmin,
};

pub const STATUS_OPEN: i32 = 0;
pub const STATUS_CLOSED: i32 = 1;
pub const STATUS_REPLIED: i32 = 1;

#[derive(Debug, Deserialize)]
pub struct TicketReplyRequest {
    pub id: i32,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct TicketCloseRequest {
    pub id: i32,
}

/// GET or POST /api/v2/admin/ticket/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<AdminTableQuery>,
    body: Option<Json<AdminTableQuery>>,
) -> Result<Response, AppError> {
    let q = body.map(|b| b.0).unwrap_or(query);

    if let Some(id) = q.id {
        let t = Ticket::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "工单不存在".to_string()))?;

        let messages = TicketMessage::find()
            .filter(ticket_message::Column::TicketId.eq(id))
            .order_by_asc(ticket_message::Column::CreatedAt)
            .all(&state.db)
            .await?;

        let user_obj = User::find_by_id(t.user_id).one(&state.db).await?;

        let mut val = serde_json::to_value(&t).unwrap_or_default();
        if let Some(obj) = val.as_object_mut() {
            obj.insert("messages".to_string(), json!(messages));
            obj.insert("user".to_string(), json!(user_obj));
        }
        return Ok(ApiResponse::success(val).into_response());
    }

    let page = q.page();
    let per_page = q.per_page();
    let offset = q.offset();

    let mut select = Ticket::find();

    for f in q.filters() {
        if f.id == "status" {
            if let Some(n) = f.value.as_i64() {
                select = select.filter(ticket::Column::Status.eq(n as i32));
            }
        } else if f.id == "subject" {
            if let Some(s) = f.value.as_str() {
                select = select.filter(ticket::Column::Subject.contains(s));
            }
        }
    }

    let total = select.clone().count(&state.db).await?;
    let tickets = select
        .order_by_desc(ticket::Column::UpdatedAt)
        .offset(offset)
        .limit(per_page)
        .all(&state.db)
        .await?;

    let mut list = Vec::new();
    for t in tickets {
        let user_obj = User::find_by_id(t.user_id).one(&state.db).await?;
        let mut val = serde_json::to_value(&t).unwrap_or_default();
        if let Some(obj) = val.as_object_mut() {
            obj.insert("user".to_string(), json!(user_obj));
        }
        list.push(val);
    }

    Ok(PaginatedResponse::new(list, total, page, per_page).into_response())
}

/// POST /api/v2/admin/ticket/reply
pub async fn reply(
    State(state): State<AppState>,
    admin: AuthenticatedAdmin,
    Json(payload): Json<TicketReplyRequest>,
) -> Result<Response, AppError> {
    let t = Ticket::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "工单不存在".to_string()))?;

    let now = chrono::Utc::now().timestamp();

    let new_msg = ticket_message::ActiveModel {
        user_id: Set(admin.0.id),
        ticket_id: Set(t.id),
        message: Set(payload.message),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    new_msg.insert(&state.db).await?;

    let mut t_active: ticket::ActiveModel = t.into();
    t_active.status = Set(STATUS_REPLIED);
    t_active.reply_status = Set(STATUS_REPLIED);
    t_active.updated_at = Set(now);
    t_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/ticket/close
pub async fn close(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<TicketCloseRequest>,
) -> Result<Response, AppError> {
    let t = Ticket::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "工单不存在".to_string()))?;

    let mut t_active: ticket::ActiveModel = t.into();
    t_active.status = Set(STATUS_CLOSED);
    t_active.updated_at = Set(chrono::Utc::now().timestamp());
    t_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}
