use crate::{
    common::{AppError, AppState},
    entities::user,
};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts},
};

/// Authenticated user extracted from `Authorization: Bearer <token>`.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub user::Model);

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        let token = match auth_header {
            Some(h) if !h.trim().is_empty() => h,
            _ => return Err(AppError::Unauthorized("authorization is null".into())),
        };

        let user = app_state
            .auth_service
            .find_user_by_bearer_token(token)
            .await?
            .ok_or_else(|| AppError::Unauthorized("authorization is expired".into()))?;

        if user.banned {
            return Err(AppError::Forbidden(
                "Your account has been suspended".into(),
            ));
        }

        Ok(AuthenticatedUser(user))
    }
}

/// Authenticated administrator extracted from `Authorization: Bearer <token>`, requires `is_admin == true`.
#[derive(Debug, Clone)]
pub struct AuthenticatedAdmin(pub user::Model);

impl<S> FromRequestParts<S> for AuthenticatedAdmin
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AuthenticatedUser(user) = AuthenticatedUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(AppError::Forbidden(
                "Access denied: Administrator privileges required".into(),
            ));
        }
        Ok(AuthenticatedAdmin(user))
    }
}
