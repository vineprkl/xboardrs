use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::common::{ApiResponse, AppError, AppState, FormOrJson as Json};

#[derive(Debug, Deserialize, Default)]
pub struct SendEmailVerifyRequest {
    pub email: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PvRequest {
    pub invite_code: Option<String>,
}

/// POST /api/v1/passport/comm/sendEmailVerify
pub async fn send_email_verify(
    State(state): State<AppState>,
    Json(payload): Json<SendEmailVerifyRequest>,
) -> Result<Response, AppError> {
    let email = match payload.email {
        Some(ref e) if !e.trim().is_empty() => e.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Email can not be empty".into(),
            ))
        }
    };

    if !email.contains('@') || !email.contains('.') {
        return Err(AppError::UnprocessableEntity(
            "Email format is incorrect".into(),
        ));
    }

    state.auth_service.send_email_verify(email).await?;
    Ok(Json(ApiResponse::success(true)).into_response())
}

/// POST /api/v1/passport/comm/pv
pub async fn pv(
    State(state): State<AppState>,
    Json(payload): Json<PvRequest>,
) -> Result<Response, AppError> {
    if let Some(code) = payload.invite_code {
        if !code.trim().is_empty() {
            state.auth_service.record_invite_pv(code.trim()).await?;
        }
    }
    Ok(Json(ApiResponse::success(true)).into_response())
}
