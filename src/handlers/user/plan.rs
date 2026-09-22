use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::EntityTrait;
use serde::Deserialize;

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::Plan,
    handlers::AuthenticatedUser,
};

#[derive(Debug, Deserialize, Default)]
pub struct UserPlanQuery {
    pub id: Option<i32>,
}

/// GET /api/v1/user/plan/fetch
pub async fn fetch(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(query): Query<UserPlanQuery>,
) -> Result<Response, AppError> {
    if let Some(id) = query.id {
        let plan = Plan::find_by_id(id).one(&state.db).await?;
        let plan = match plan {
            Some(p) => p,
            None => {
                return Err(AppError::BadRequest(
                    "Subscription plan does not exist".into(),
                ))
            }
        };

        if !state.user_service.is_plan_available_for_user(&plan, &user) {
            return Err(AppError::BadRequest(
                "Subscription plan does not exist".into(),
            ));
        }

        let plan_dto = state.plan_service.format_plan(&plan);
        return Ok(Json(ApiResponse::success(plan_dto)).into_response());
    }

    let plans = state.plan_service.get_available_plans().await?;
    Ok(Json(ApiResponse::success(plans)).into_response())
}
