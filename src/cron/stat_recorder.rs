use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use crate::{
    common::{AppError, AppState},
    entities::{
        commission_log, order, stat::overall as stat_overall, user, CommissionLog, Order, Stat,
        User,
    },
    services::order_service::STATUS_COMPLETED,
};

/// Records daily statistics matching PHP `XboardStatistics`.
/// Aggregates yesterday's data and writes or updates `v2_stat`.
pub async fn record_daily_stats(state: &AppState) -> Result<stat_overall::Model, AppError> {
    let now = Utc::now();
    let today_date = NaiveDate::from_ymd_opt(now.year(), now.month(), now.day())
        .ok_or_else(|| AppError::Internal("Invalid date".to_string()))?;
    let end_at = Utc
        .from_utc_datetime(&today_date.and_hms_opt(0, 0, 0).unwrap())
        .timestamp();
    let start_at = end_at - 86400; // yesterday 00:00:00

    // 1. Orders
    let completed_orders = Order::find()
        .filter(order::Column::Status.eq(STATUS_COMPLETED))
        .filter(order::Column::CreatedAt.gte(start_at))
        .filter(order::Column::CreatedAt.lt(end_at))
        .all(&state.db)
        .await?;

    let order_count = completed_orders.len() as i32;
    let order_total: i32 = completed_orders.iter().map(|o| o.total_amount).sum();
    let paid_count = order_count;
    let paid_total = order_total;

    // 2. Commissions
    let commission_logs = CommissionLog::find()
        .filter(commission_log::Column::CreatedAt.gte(start_at))
        .filter(commission_log::Column::CreatedAt.lt(end_at))
        .all(&state.db)
        .await?;

    let commission_count = commission_logs.len() as i32;
    let commission_total: i32 = commission_logs.iter().map(|c| c.get_amount).sum();

    // 3. Registrations
    let register_count = User::find()
        .filter(user::Column::CreatedAt.gte(start_at))
        .filter(user::Column::CreatedAt.lt(end_at))
        .count(&state.db)
        .await? as i32;

    let invite_count = User::find()
        .filter(user::Column::InviteUserId.is_not_null())
        .filter(user::Column::CreatedAt.gte(start_at))
        .filter(user::Column::CreatedAt.lt(end_at))
        .count(&state.db)
        .await? as i32;

    // 4. Save or update
    let existing = Stat::find()
        .filter(stat_overall::Column::RecordAt.eq(start_at))
        .filter(stat_overall::Column::RecordType.eq("d"))
        .one(&state.db)
        .await?;

    let now_ts = Utc::now().timestamp();

    if let Some(model) = existing {
        let mut active: stat_overall::ActiveModel = model.into();
        active.order_count = Set(order_count);
        active.order_total = Set(order_total);
        active.paid_count = Set(paid_count);
        active.paid_total = Set(paid_total);
        active.commission_count = Set(commission_count);
        active.commission_total = Set(commission_total);
        active.register_count = Set(register_count);
        active.invite_count = Set(invite_count);
        active.updated_at = Set(now_ts);
        Ok(active.update(&state.db).await?)
    } else {
        let new_stat = stat_overall::ActiveModel {
            record_at: Set(start_at),
            record_type: Set("d".to_string()),
            order_count: Set(order_count),
            order_total: Set(order_total),
            paid_count: Set(paid_count),
            paid_total: Set(paid_total),
            commission_count: Set(commission_count),
            commission_total: Set(commission_total),
            register_count: Set(register_count),
            invite_count: Set(invite_count),
            transfer_used_total: Set("0".to_string()),
            created_at: Set(now_ts),
            updated_at: Set(now_ts),
            ..Default::default()
        };
        Ok(new_stat.insert(&state.db).await?)
    }
}
