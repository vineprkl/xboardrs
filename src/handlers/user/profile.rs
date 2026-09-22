use axum::{
    extract::State,
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError, AppState, FormOrJson as Json},
    entities::{personal_access_token, PersonalAccessToken, Plan},
    handlers::AuthenticatedUser,
    services::UserService,
    utils::get_subscribe_url,
};

#[derive(Debug, Deserialize, Default)]
pub struct ChangePasswordRequest {
    pub old_password: Option<String>,
    pub new_password: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UserUpdateRequest {
    pub remind_expire: Option<bool>,
    pub remind_traffic: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TransferRequest {
    pub transfer_amount: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
pub struct QuickLoginUrlRequest {
    pub redirect: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RemoveSessionRequest {
    pub session_id: Option<i64>,
}

/// GET /api/v1/user/info
pub async fn info(AuthenticatedUser(user): AuthenticatedUser) -> Result<Response, AppError> {
    let avatar_url = UserService::get_avatar_url(&user.email);
    let data = json!({
        "email": user.email,
        "transfer_enable": user.transfer_enable,
        "last_login_at": user.last_login_at,
        "created_at": user.created_at,
        "banned": user.banned,
        "remind_expire": user.remind_expire,
        "remind_traffic": user.remind_traffic,
        "expired_at": user.expired_at,
        "balance": user.balance,
        "commission_balance": user.commission_balance,
        "plan_id": user.plan_id,
        "discount": user.discount,
        "commission_rate": user.commission_rate,
        "telegram_id": user.telegram_id,
        "uuid": user.uuid,
        "avatar_url": avatar_url,
    });
    Ok(Json(ApiResponse::success(data)).into_response())
}

/// POST /api/v1/user/changePassword
pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Response, AppError> {
    let old_pwd = match payload.old_password {
        Some(ref p) if !p.trim().is_empty() => p.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Password can not be empty".into(),
            ))
        }
    };

    let new_pwd = match payload.new_password {
        Some(ref p) if !p.trim().is_empty() => p.trim(),
        _ => {
            return Err(AppError::UnprocessableEntity(
                "Password can not be empty".into(),
            ))
        }
    };

    if new_pwd.len() < 8 {
        return Err(AppError::UnprocessableEntity(
            "Password must be greater than 8 digits".into(),
        ));
    }

    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    state
        .user_service
        .change_password(&state.db, &user, old_pwd, new_pwd, auth_header)
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}

/// POST /api/v1/user/update
pub async fn update(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<UserUpdateRequest>,
) -> Result<Response, AppError> {
    state
        .user_service
        .update_remind_settings(
            &state.db,
            &user,
            payload.remind_expire,
            payload.remind_traffic,
        )
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}

/// GET /api/v1/user/resetSecurity
pub async fn reset_security(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Response, AppError> {
    let (_new_uuid, new_token) = state.user_service.reset_security(&state.db, &user).await?;
    let base_url = state.setting_service.get_string("app_url", "").await;
    let subscribe_path = state
        .setting_service
        .get_string("subscribe_path", "s")
        .await;
    let url = get_subscribe_url(&base_url, &subscribe_path, &new_token);

    Ok(Json(ApiResponse::success(url)).into_response())
}

/// GET /api/v1/user/getSubscribe
pub async fn get_subscribe(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Response, AppError> {
    let plan = if let Some(pid) = user.plan_id {
        Plan::find_by_id(pid).one(&state.db).await?
    } else {
        None
    };

    let base_url = state.setting_service.get_string("app_url", "").await;
    let subscribe_path = state
        .setting_service
        .get_string("subscribe_path", "s")
        .await;
    let subscribe_url = get_subscribe_url(&base_url, &subscribe_path, &user.token);
    let reset_day = state.user_service.get_reset_day(&user, plan.as_ref());

    let plan_json = plan.as_ref().map(|p| {
        json!({
            "id": p.id,
            "group_id": p.group_id,
            "transfer_enable": p.transfer_enable,
            "name": p.name,
            "speed_limit": p.speed_limit,
            "show": p.show,
            "sort": p.sort,
            "renew": p.renew,
            "reset_traffic_method": p.reset_traffic_method,
            "capacity_limit": p.capacity_limit,
            "created_at": p.created_at,
            "updated_at": p.updated_at,
        })
    });

    let data = json!({
        "plan_id": user.plan_id,
        "token": user.token,
        "expired_at": user.expired_at,
        "u": user.u,
        "d": user.d,
        "transfer_enable": user.transfer_enable,
        "email": user.email,
        "uuid": user.uuid,
        "device_limit": user.device_limit,
        "speed_limit": user.speed_limit,
        "next_reset_at": user.next_reset_at,
        "plan": plan_json,
        "subscribe_url": subscribe_url,
        "reset_day": reset_day,
    });

    Ok(Json(ApiResponse::success(data)).into_response())
}

/// GET /api/v1/user/getStat
pub async fn get_stat(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Response, AppError> {
    let stats = state.user_service.get_user_stat(&state.db, user.id).await?;
    Ok(Json(ApiResponse::success(stats)).into_response())
}

/// GET /api/v1/user/checkLogin
pub async fn check_login(AuthenticatedUser(user): AuthenticatedUser) -> Result<Response, AppError> {
    let mut data = json!({
        "is_login": true,
    });
    if user.is_admin {
        data["is_admin"] = json!(true);
    }
    Ok(Json(ApiResponse::success(data)).into_response())
}

/// POST /api/v1/user/transfer
pub async fn transfer(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<TransferRequest>,
) -> Result<Response, AppError> {
    let amount = payload
        .transfer_amount
        .ok_or_else(|| AppError::UnprocessableEntity("transfer_amount is required".into()))?;

    state
        .user_service
        .transfer_commission(&state.db, user.id, amount)
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}

/// POST /api/v1/user/getQuickLoginUrl
pub async fn get_quick_login_url(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<QuickLoginUrlRequest>,
) -> Result<Response, AppError> {
    let url = state
        .auth_service
        .generate_quick_login_url(&user, payload.redirect.as_deref())
        .await?;
    Ok(Json(ApiResponse::success(url)).into_response())
}

/// GET /api/v1/user/getActiveSession
pub async fn get_active_session(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Response, AppError> {
    let sessions = PersonalAccessToken::find()
        .filter(personal_access_token::Column::TokenableId.eq(user.id))
        .all(&state.db)
        .await?;

    Ok(Json(ApiResponse::success(sessions)).into_response())
}

/// POST /api/v1/user/removeActiveSession
pub async fn remove_active_session(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Json(payload): Json<RemoveSessionRequest>,
) -> Result<Response, AppError> {
    let session_id = payload
        .session_id
        .ok_or_else(|| AppError::UnprocessableEntity("session_id is required".into()))?;

    PersonalAccessToken::delete_many()
        .filter(personal_access_token::Column::TokenableId.eq(user.id))
        .filter(personal_access_token::Column::Id.eq(session_id))
        .exec(&state.db)
        .await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}
