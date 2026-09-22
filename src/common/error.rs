use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation failed: {0}")]
    Validation(String, Option<serde_json::Value>),

    #[error("Unprocessable entity: {0}")]
    UnprocessableEntity(String),

    #[error("Custom error {0}: {1}")]
    Custom(i32, String),

    #[error("Too many requests: {0}")]
    TooManyRequests(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("Unexpected error: {0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<()>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<serde_json::Value>,
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Validation(_, _) | Self::UnprocessableEntity(_) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::Custom(code, _) => {
                let status_val = if *code >= 100000 { *code / 1000 } else { *code };
                if (100..600).contains(&status_val) {
                    StatusCode::from_u16(status_val as u16).unwrap_or(StatusCode::BAD_REQUEST)
                } else {
                    StatusCode::BAD_REQUEST
                }
            }
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) | Self::Database(_) | Self::Other(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    pub fn error_message(&self) -> String {
        match self {
            Self::BadRequest(msg) => msg.clone(),
            Self::Unauthorized(msg) => msg.clone(),
            Self::Forbidden(msg) => msg.clone(),
            Self::NotFound(msg) => msg.clone(),
            Self::Validation(msg, _) => msg.clone(),
            Self::UnprocessableEntity(msg) => msg.clone(),
            Self::Custom(_, msg) => msg.clone(),
            Self::TooManyRequests(msg) => msg.clone(),
            Self::Internal(msg) => msg.clone(),
            Self::Database(err) => {
                tracing::error!("Database error: {:?}", err);
                "数据库操作异常".to_string()
            }
            Self::Other(err) => {
                tracing::error!("Unexpected error: {:?}", err);
                "服务器内部异常".to_string()
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let code = self.status_code();
        let message = self.error_message();

        let (error, errors) = match &self {
            Self::Validation(_, details) => (details.clone(), details.clone()),
            _ => (None, None),
        };

        let body = ErrorResponse {
            status: "fail".to_string(),
            message,
            data: None,
            error,
            errors,
        };

        (code, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            AppError::BadRequest("invalid".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AppError::Unauthorized("login".into()).status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            AppError::Forbidden("no perm".into()).status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            AppError::NotFound("missing".into()).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::Validation("check".into(), None).status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            AppError::Internal("fail".into()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_response_serialization() {
        let err = AppError::BadRequest("请求参数有误".into());
        let code = err.status_code();
        assert_eq!(code, StatusCode::BAD_REQUEST);

        let body = ErrorResponse {
            status: "fail".to_string(),
            message: err.error_message(),
            data: None,
            error: None,
            errors: None,
        };

        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains(r#""status":"fail""#));
        assert!(json.contains(r#""message":"请求参数有误""#));
    }
}
