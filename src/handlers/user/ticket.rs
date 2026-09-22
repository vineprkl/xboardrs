use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    common::{ApiResponse, AppError, AppState, FormOrJson as Json},
    handlers::AuthenticatedUser,
};

#[derive(Debug, Deserialize, Default)]
pub struct TicketFetchQuery {
    pub id: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TicketSaveRequest {
    pub subject: Option<String>,
    pub level: Option<i32>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TicketReplyRequest {
    pub id: Option<i32>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TicketCloseRequest {
    pub id: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TicketWithdrawRequest {
    pub withdraw_method: Option<String>,
    pub withdraw_account: Option<String>,
}

const DEFAULT_WITHDRAW_METHODS: &[&str] = &["alipay", "usdt"];

/// GET /api/v1/user/ticket/fetch
pub async fn fetch(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(query): Query<TicketFetchQuery>,
) -> Result<Response, AppError> {
    if let Some(id) = query.id {
        let detail = state
            .ticket_service
            .fetch_ticket_detail(user.id, id)
            .await?;
        return Ok(Json(ApiResponse::success(detail)).into_response());
    }

    let tickets = state.ticket_service.fetch_user_tickets(user.id).await?;
    Ok(Json(ApiResponse::success(tickets)).into_response())
}

/// POST /api/v1/user/ticket/save
pub async fn save(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<TicketSaveRequest>,
) -> Result<Response, AppError> {
    let subject = match payload.subject {
        Some(ref s) if !s.trim().is_empty() => s.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Subject cannot be empty".into(),
            ))
        }
    };

    let level = payload.level.unwrap_or(2); // 1: low, 2: medium, 3: high

    let message = match payload.message {
        Some(ref m) if !m.trim().is_empty() => m.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Message cannot be empty".into(),
            ))
        }
    };

    state
        .ticket_service
        .create_ticket(user.id, subject, level, message)
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}

/// POST /api/v1/user/ticket/reply
pub async fn reply(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<TicketReplyRequest>,
) -> Result<Response, AppError> {
    let ticket_id = payload
        .id
        .ok_or_else(|| AppError::BadRequest("Invalid parameter".into()))?;

    let message = match payload.message {
        Some(ref m) if !m.trim().is_empty() => m.trim(),
        _ => return Err(AppError::BadRequest("Message cannot be empty".into())),
    };

    let must_wait_reply = state
        .setting_service
        .get_bool("ticket_must_wait_reply", false)
        .await;

    state
        .ticket_service
        .reply_ticket(user.id, ticket_id, message, must_wait_reply)
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}

/// POST /api/v1/user/ticket/close
pub async fn close(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<TicketCloseRequest>,
) -> Result<Response, AppError> {
    let ticket_id = payload
        .id
        .ok_or_else(|| AppError::UnprocessableEntity("Invalid parameter".into()))?;

    state
        .ticket_service
        .close_ticket(user.id, ticket_id)
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}

/// POST /api/v1/user/ticket/withdraw
pub async fn withdraw(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<TicketWithdrawRequest>,
) -> Result<Response, AppError> {
    if state
        .setting_service
        .get_bool("withdraw_close_enable", false)
        .await
    {
        return Err(AppError::BadRequest("Unsupported withdraw".into()));
    }

    let method = match payload.withdraw_method {
        Some(ref m) if !m.trim().is_empty() => m.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Unsupported withdrawal method".into(),
            ))
        }
    };

    let account = match payload.withdraw_account {
        Some(ref a) if !a.trim().is_empty() => a.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Withdrawal account cannot be empty".into(),
            ))
        }
    };

    let allowed_methods_raw = state
        .setting_service
        .get_string("commission_withdraw_method", "")
        .await;
    let allowed_methods: Vec<String> = if allowed_methods_raw.trim().is_empty() {
        DEFAULT_WITHDRAW_METHODS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else if let Ok(arr) = serde_json::from_str::<Vec<String>>(&allowed_methods_raw) {
        arr
    } else {
        allowed_methods_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };

    if !allowed_methods
        .iter()
        .any(|m| m.eq_ignore_ascii_case(method))
    {
        return Err(AppError::UnprocessableEntity(
            "Unsupported withdrawal method".into(),
        ));
    }

    let min_limit = state
        .setting_service
        .get_int("commission_withdraw_limit", 100)
        .await;
    if min_limit > (user.commission_balance as i64 / 100) {
        return Err(AppError::UnprocessableEntity(format!(
            "The current required minimum withdrawal commission is {}",
            min_limit
        )));
    }

    let subject = "[Commission Withdrawal Request] This ticket is opened by the system";
    let body = format!(
        "Withdrawal method: {}\r\nWithdrawal account: {}",
        method, account
    );

    state
        .ticket_service
        .create_ticket(user.id, subject, 2, &body)
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}
