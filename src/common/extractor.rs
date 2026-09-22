use axum::{
    body::to_bytes,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
    Json as AxumJson,
};
use serde::{de::DeserializeOwned, Serialize};
use std::ops::{Deref, DerefMut};

use crate::common::AppError;

/// An extractor and response type that accepts both JSON and URL-encoded forms.
/// Compatible with Laravel's unified `$request->input(...)` handling, preventing
/// HTTP 415 Unsupported Media Type when web frontends submit form-urlencoded data.
#[derive(Debug, Clone, Copy, Default)]
pub struct FormOrJson<T>(pub T);

impl<T> Deref for FormOrJson<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for FormOrJson<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for FormOrJson<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

impl<T: Serialize> IntoResponse for FormOrJson<T> {
    fn into_response(self) -> Response {
        AxumJson(self.0).into_response()
    }
}

impl<S, T> FromRequest<S> for FormOrJson<T>
where
    T: DeserializeOwned + 'static,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let (parts, body) = req.into_parts();
        let bytes = to_bytes(body, 10 * 1024 * 1024)
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read request body: {}", e)))?;

        if bytes.is_empty() {
            if let Ok(val) = serde_json::from_str::<T>("{}") {
                return Ok(FormOrJson(val));
            }
            if let Ok(val) = serde_urlencoded::from_bytes::<T>(&[]) {
                return Ok(FormOrJson(val));
            }
            return Err(AppError::UnprocessableEntity(
                "Request body cannot be empty".into(),
            ));
        }

        // 1. Try parsing as JSON first
        if let Ok(val) = serde_json::from_slice::<T>(&bytes) {
            return Ok(FormOrJson(val));
        }

        // 2. Try parsing as URL-encoded form
        if let Ok(val) = serde_urlencoded::from_bytes::<T>(&bytes) {
            return Ok(FormOrJson(val));
        }

        // 3. Fallback error with context
        let content_type = parts
            .headers
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if content_type.contains("application/json") {
            if let Err(err) = serde_json::from_slice::<T>(&bytes) {
                return Err(AppError::UnprocessableEntity(format!(
                    "Invalid JSON payload: {}",
                    err
                )));
            }
        } else if let Err(err) = serde_urlencoded::from_bytes::<T>(&bytes) {
            return Err(AppError::UnprocessableEntity(format!(
                "Invalid form payload: {}",
                err
            )));
        }

        Err(AppError::UnprocessableEntity(
            "Invalid request payload".into(),
        ))
    }
}
