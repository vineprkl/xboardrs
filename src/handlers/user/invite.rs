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
    common::{ApiResponse, AppError, AppState},
    entities::{commission_log, invite_code, order, user, CommissionLog, InviteCode, Order, User},
    handlers::AuthenticatedUser,
    utils::random_char,
};

#[derive(Debug, Deserialize, Default)]
pub struct InviteDetailsQuery {
    pub current: Option<u64>,
    pub page_size: Option<u64>,
}

/// GET /api/v1/user/invite/save
pub async fn save(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Response, AppError> {
    let limit = state.setting_service.get_int("invite_gen_limit", 5).await;
    let unused_count = InviteCode::find()
        .filter(invite_code::Column::UserId.eq(user.id))
        .filter(invite_code::Column::Status.eq(false))
        .count(&state.db)
        .await?;

    if unused_count >= limit as u64 {
        return Err(AppError::BadRequest(
            "The maximum number of creations has been reached".into(),
        ));
    }

    let code_str = random_char(8, false);
    let now = chrono::Utc::now().timestamp();

    let new_invite = invite_code::ActiveModel {
        user_id: Set(user.id),
        code: Set(code_str),
        status: Set(false),
        pv: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    new_invite.insert(&state.db).await?;

    Ok(Json(ApiResponse::success(true)).into_response())
}

/// GET /api/v1/user/invite/fetch
pub async fn fetch(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> Result<Response, AppError> {
    let default_commission = state.setting_service.get_int("invite_commission", 10).await;
    let commission_rate = user.commission_rate.unwrap_or(default_commission as f64) as i64;

    let codes = InviteCode::find()
        .filter(invite_code::Column::UserId.eq(user.id))
        .filter(invite_code::Column::Status.eq(false))
        .order_by_desc(invite_code::Column::CreatedAt)
        .all(&state.db)
        .await?;

    let invited_users_count = User::find()
        .filter(user::Column::InviteUserId.eq(user.id))
        .count(&state.db)
        .await? as i64;

    // Valid commission sum from CommissionLog
    let valid_commission: i64 = CommissionLog::find()
        .filter(commission_log::Column::InviteUserId.eq(user.id))
        .select_only()
        .column_as(commission_log::Column::GetAmount.sum(), "sum_amount")
        .into_tuple::<Option<i64>>()
        .one(&state.db)
        .await?
        .flatten()
        .unwrap_or(0);

    // Unchecked commission from pending orders (status = 3, commission_status = 0)
    let uncheck_commission: i64 = Order::find()
        .filter(order::Column::Status.eq(3))
        .filter(order::Column::CommissionStatus.eq(0))
        .filter(order::Column::InviteUserId.eq(user.id))
        .select_only()
        .column_as(order::Column::CommissionBalance.sum(), "sum_comm")
        .into_tuple::<Option<i64>>()
        .one(&state.db)
        .await?
        .flatten()
        .unwrap_or(0);

    let final_uncheck = if state
        .setting_service
        .get_bool("commission_distribution_enable", false)
        .await
    {
        let l1 = state
            .setting_service
            .get_int("commission_distribution_l1", 100)
            .await as f64;
        ((uncheck_commission as f64) * (l1 / 100.0)).round() as i64
    } else {
        uncheck_commission
    };

    let stat = [
        invited_users_count,
        valid_commission,
        final_uncheck,
        commission_rate,
        user.commission_balance as i64,
    ];

    let data = json!({
        "codes": codes,
        "stat": stat,
    });

    Ok(Json(ApiResponse::success(data)).into_response())
}

/// GET /api/v1/user/invite/details
pub async fn details(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(query): Query<InviteDetailsQuery>,
) -> Result<Response, AppError> {
    let current = query.current.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(10).max(10);

    let q = CommissionLog::find()
        .filter(commission_log::Column::InviteUserId.eq(user.id))
        .filter(commission_log::Column::GetAmount.gt(0))
        .order_by_desc(commission_log::Column::CreatedAt);

    let paginator = q.paginate(&state.db, page_size);
    let total = paginator.num_items().await?;
    let items = paginator.fetch_page(current - 1).await?;

    let resp = json!({
        "data": items,
        "total": total,
    });

    Ok(Json(resp).into_response())
}
