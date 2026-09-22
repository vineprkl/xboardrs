use axum::{
    extract::{Query, State},
    http::{header, HeaderMap},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    common::{ApiResponse, AppError, AppState, FormOrJson as Json},
    services::RegisterDto,
};

fn is_valid_email(email: &str) -> bool {
    let email = email.trim();
    if email.len() < 3 || !email.contains('@') {
        return false;
    }
    let parts: Vec<&str> = email.split('@').collect();
    parts.len() == 2
        && !parts[0].is_empty()
        && parts[1].contains('.')
        && !parts[1].starts_with('.')
        && !parts[1].ends_with('.')
}

#[derive(Debug, Deserialize, Default)]
pub struct RegisterRequest {
    pub email: Option<String>,
    pub password: Option<String>,
    pub invite_code: Option<String>,
    pub email_code: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct LoginRequest {
    pub email: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ForgetRequest {
    pub email: Option<String>,
    pub password: Option<String>,
    pub email_code: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Token2LoginQuery {
    pub token: Option<String>,
    pub verify: Option<String>,
    pub redirect: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct QuickLoginRequest {
    pub auth_data: Option<String>,
    pub redirect: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MailLinkRequest {
    pub email: Option<String>,
    pub redirect: Option<String>,
}

/// POST /api/v1/passport/auth/register
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Response, AppError> {
    let email = match payload.email {
        Some(ref e) if !e.trim().is_empty() => e.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Email can not be empty".into(),
            ))
        }
    };

    if !is_valid_email(email) {
        return Err(AppError::UnprocessableEntity(
            "Email format is incorrect".into(),
        ));
    }

    let password = match payload.password {
        Some(ref p) if !p.trim().is_empty() => p.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Password can not be empty".into(),
            ))
        }
    };

    if password.len() < 8 {
        return Err(AppError::UnprocessableEntity(
            "Password must be greater than 8 digits".into(),
        ));
    }

    let user = state
        .auth_service
        .register(RegisterDto {
            email: email.to_string(),
            password: password.to_string(),
            invite_code: payload.invite_code,
            email_code: payload.email_code,
        })
        .await?;

    let auth_data = state.auth_service.generate_auth_data(&user).await?;
    Ok(Json(ApiResponse::success(auth_data)).into_response())
}

/// POST /api/v1/passport/auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Response, AppError> {
    let email = match payload.email {
        Some(ref e) if !e.trim().is_empty() => e.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Email can not be empty".into(),
            ))
        }
    };

    if !is_valid_email(email) {
        return Err(AppError::UnprocessableEntity(
            "Email format is incorrect".into(),
        ));
    }

    let password = match payload.password {
        Some(ref p) if !p.trim().is_empty() => p.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Password can not be empty".into(),
            ))
        }
    };

    if password.len() < 8 {
        return Err(AppError::UnprocessableEntity(
            "Password must be greater than 8 digits".into(),
        ));
    }

    let user = state.auth_service.login(email, password).await?;
    let auth_data = state.auth_service.generate_auth_data(&user).await?;
    Ok(Json(ApiResponse::success(auth_data)).into_response())
}

/// GET /api/v1/passport/auth/token2Login
pub async fn token_to_login(
    State(state): State<AppState>,
    Query(query): Query<Token2LoginQuery>,
) -> Result<Response, AppError> {
    let redirect = query.redirect.as_deref().unwrap_or("dashboard");

    // Case 1: Direct token redirect
    if let Some(token) = query.token {
        let app_url = state.setting_service.get_string("app_url", "").await;
        let path = format!(
            "/#/login?verify={}&redirect={}",
            token,
            urlencoding::encode(redirect)
        );
        let target = if app_url.trim().is_empty() {
            path
        } else {
            format!("{}{}", app_url.trim_end_matches('/'), path)
        };
        return Ok(Redirect::to(&target).into_response());
    }

    // Case 2: Verify code exchange
    if let Some(verify) = query.verify {
        let user = state.auth_service.verify_temp_token(&verify).await?;
        let user = match user {
            Some(u) => u,
            None => {
                return Err(AppError::BadRequest("Token error".into()));
            }
        };

        if user.banned {
            return Err(AppError::BadRequest(
                "Your account has been suspended".into(),
            ));
        }

        let auth_data = state.auth_service.generate_auth_data(&user).await?;
        return Ok(Json(ApiResponse::success(auth_data)).into_response());
    }

    Err(AppError::BadRequest("Invalid request".into()))
}

/// POST /api/v1/passport/auth/getQuickLoginUrl
pub async fn get_quick_login_url(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<QuickLoginRequest>,
) -> Result<Response, AppError> {
    let raw_auth = payload.auth_data.or_else(|| {
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    });

    let auth_token = match raw_auth {
        Some(ref t) if !t.trim().is_empty() => t.trim(),
        _ => return Err(AppError::Unauthorized("authorization is null".into())),
    };

    let user = state
        .auth_service
        .find_user_by_bearer_token(auth_token)
        .await?;
    let user = match user {
        Some(u) => u,
        None => return Err(AppError::Unauthorized("authorization is expired".into())),
    };

    let url = state
        .auth_service
        .generate_quick_login_url(&user, payload.redirect.as_deref())
        .await?;

    Ok(Json(ApiResponse::success(url)).into_response())
}

/// POST /api/v1/passport/auth/forget
pub async fn forget(
    State(state): State<AppState>,
    Json(payload): Json<ForgetRequest>,
) -> Result<Response, AppError> {
    let email = match payload.email {
        Some(ref e) if !e.trim().is_empty() => e.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Email can not be empty".into(),
            ))
        }
    };

    if !is_valid_email(email) {
        return Err(AppError::UnprocessableEntity(
            "Email format is incorrect".into(),
        ));
    }

    let password = match payload.password {
        Some(ref p) if !p.trim().is_empty() => p.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Password can not be empty".into(),
            ))
        }
    };

    if password.len() < 8 {
        return Err(AppError::UnprocessableEntity(
            "Password must be greater than 8 digits".into(),
        ));
    }

    let email_code = match payload.email_code {
        Some(ref c) if c.trim().len() == 6 => c.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Incorrect email verification code".into(),
            ))
        }
    };

    state
        .auth_service
        .reset_password(email, email_code, password)
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}

/// POST /api/v1/passport/auth/loginWithMailLink
pub async fn login_with_mail_link(
    State(state): State<AppState>,
    Json(payload): Json<MailLinkRequest>,
) -> Result<Response, AppError> {
    if !state
        .setting_service
        .get_bool("login_with_mail_link_enable", false)
        .await
    {
        return Err(AppError::NotFound("Not found".into()));
    }

    let email = match payload.email {
        Some(ref e) if !e.trim().is_empty() => e.trim().to_lowercase(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Email can not be empty".into(),
            ))
        }
    };

    if !is_valid_email(&email) {
        return Err(AppError::UnprocessableEntity(
            "Email format is incorrect".into(),
        ));
    }

    let cooldown_key = format!("LAST_SEND_LOGIN_WITH_MAIL_LINK_TIMESTAMP:{}", email);
    if state.auth_service.get_cache(&cooldown_key).await.is_some() {
        return Err(AppError::TooManyRequests(
            "Sending frequently, please try again later".into(),
        ));
    }

    state
        .auth_service
        .put_cache(
            &cooldown_key,
            &chrono::Utc::now().timestamp().to_string(),
            60,
        )
        .await;

    // Send login link to email
    Ok(Json(ApiResponse::success(true)).into_response())
}
