use chrono::{Datelike, NaiveDate, TimeZone, Timelike, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait};

use crate::{
    common::{AppError, AppState},
    entities::{traffic_reset_log, user, Plan, User},
};

pub const RESET_TRAFFIC_FIRST_DAY_MONTH: i32 = 0;
pub const RESET_TRAFFIC_MONTHLY: i32 = 1;
pub const RESET_TRAFFIC_NEVER: i32 = 2;
pub const RESET_TRAFFIC_FIRST_DAY_YEAR: i32 = 3;
pub const RESET_TRAFFIC_YEARLY: i32 = 4;
pub const RESET_TRAFFIC_FOLLOW_SYSTEM: i32 = -1;

/// Calculates the next reset timestamp for a user based on their expiration date and plan's reset method.
/// Exactly matches PHP `TrafficResetService::calculateNextResetTime`.
pub fn calculate_next_reset_time(
    expired_at: Option<i64>,
    plan_reset_method: Option<i32>,
    system_reset_method: i32,
    now_ts: i64,
) -> Option<i64> {
    let method = match plan_reset_method {
        Some(RESET_TRAFFIC_FOLLOW_SYSTEM) | None => system_reset_method,
        Some(m) => m,
    };

    if method == RESET_TRAFFIC_NEVER {
        return None;
    }

    let now_dt = match Utc.timestamp_opt(now_ts, 0) {
        chrono::LocalResult::Single(dt) => dt,
        _ => return None,
    };

    match method {
        RESET_TRAFFIC_FIRST_DAY_MONTH => {
            // First day of next month at 00:00:00 UTC
            let (next_year, next_month) = if now_dt.month() == 12 {
                (now_dt.year() + 1, 1)
            } else {
                (now_dt.year(), now_dt.month() + 1)
            };
            let next_date = NaiveDate::from_ymd_opt(next_year, next_month, 1)?;
            let next_dt = next_date.and_hms_opt(0, 0, 0)?;
            Some(Utc.from_utc_datetime(&next_dt).timestamp())
        }
        RESET_TRAFFIC_MONTHLY => {
            let exp_dt = match expired_at {
                Some(ts) => match Utc.timestamp_opt(ts, 0) {
                    chrono::LocalResult::Single(dt) => dt,
                    _ => now_dt,
                },
                None => now_dt,
            };

            let reset_day = exp_dt.day();
            let (hour, minute, second) = (exp_dt.hour(), exp_dt.minute(), exp_dt.second());

            // 1. Check if reset day in current month is in the future
            let last_day_curr_month = days_in_month(now_dt.year(), now_dt.month());
            let target_day_curr = reset_day.min(last_day_curr_month);
            if let Some(d) = NaiveDate::from_ymd_opt(now_dt.year(), now_dt.month(), target_day_curr)
            {
                if let Some(dt) = d.and_hms_opt(hour, minute, second) {
                    let ts = Utc.from_utc_datetime(&dt).timestamp();
                    if ts > now_ts {
                        return Some(ts);
                    }
                }
            }

            // 2. Otherwise, calculate for next month
            let (next_year, next_month) = if now_dt.month() == 12 {
                (now_dt.year() + 1, 1)
            } else {
                (now_dt.year(), now_dt.month() + 1)
            };
            let last_day_next_month = days_in_month(next_year, next_month);
            let target_day = reset_day.min(last_day_next_month);

            let next_date = NaiveDate::from_ymd_opt(next_year, next_month, target_day)?;
            let next_dt = next_date.and_hms_opt(hour, minute, second)?;
            Some(Utc.from_utc_datetime(&next_dt).timestamp())
        }
        RESET_TRAFFIC_FIRST_DAY_YEAR => {
            // First day of next year at 00:00:00 UTC
            let next_date = NaiveDate::from_ymd_opt(now_dt.year() + 1, 1, 1)?;
            let next_dt = next_date.and_hms_opt(0, 0, 0)?;
            Some(Utc.from_utc_datetime(&next_dt).timestamp())
        }
        RESET_TRAFFIC_YEARLY => {
            let exp_dt = match expired_at {
                Some(ts) => match Utc.timestamp_opt(ts, 0) {
                    chrono::LocalResult::Single(dt) => dt,
                    _ => now_dt,
                },
                None => now_dt,
            };

            let reset_month = exp_dt.month();
            let reset_day = exp_dt.day();
            let (hour, minute, second) = (exp_dt.hour(), exp_dt.minute(), exp_dt.second());

            // 1. Check if reset date in current year is in the future
            let last_day_curr = days_in_month(now_dt.year(), reset_month);
            let target_day_curr = reset_day.min(last_day_curr);
            if let Some(d) = NaiveDate::from_ymd_opt(now_dt.year(), reset_month, target_day_curr) {
                if let Some(dt) = d.and_hms_opt(hour, minute, second) {
                    let ts = Utc.from_utc_datetime(&dt).timestamp();
                    if ts > now_ts {
                        return Some(ts);
                    }
                }
            }

            // 2. Otherwise, next year
            let next_year = now_dt.year() + 1;
            let last_day_next = days_in_month(next_year, reset_month);
            let target_day = reset_day.min(last_day_next);

            let next_date = NaiveDate::from_ymd_opt(next_year, reset_month, target_day)?;
            let next_dt = next_date.and_hms_opt(hour, minute, second)?;
            Some(Utc.from_utc_datetime(&next_dt).timestamp())
        }
        _ => None,
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Executes traffic reset for all eligible users.
/// Returns the number of users whose traffic was reset.
pub async fn check_and_reset_traffic(state: &AppState) -> Result<usize, AppError> {
    let now = Utc::now().timestamp();
    let system_reset_method = state
        .setting_service
        .get_int("reset_traffic_method", RESET_TRAFFIC_MONTHLY as i64)
        .await as i32;

    // Users who are active, have a plan, not banned, and next_reset_at <= now
    let users = User::find()
        .filter(user::Column::Banned.eq(false))
        .filter(user::Column::PlanId.is_not_null())
        .filter(user::Column::NextResetAt.is_not_null())
        .filter(user::Column::NextResetAt.lte(now))
        .all(&state.db)
        .await?;

    let mut reset_count = 0;

    for u in users {
        // Skip expired users
        if let Some(exp) = u.expired_at {
            if exp <= now {
                continue;
            }
        }

        let plan_opt = if let Some(pid) = u.plan_id {
            Plan::find_by_id(pid).one(&state.db).await?
        } else {
            None
        };

        let plan_reset_method = plan_opt.and_then(|p| p.reset_traffic_method);
        let next_reset_at =
            calculate_next_reset_time(u.expired_at, plan_reset_method, system_reset_method, now);

        let old_upload = u.u;
        let old_download = u.d;
        let old_total = old_upload + old_download;

        let txn = state.db.begin().await?;

        let mut u_active: user::ActiveModel = u.clone().into();
        u_active.u = Set(0);
        u_active.d = Set(0);
        u_active.last_reset_at = Set(Some(now));
        u_active.reset_count = Set(Some(u.reset_count.unwrap_or(0) + 1));
        u_active.next_reset_at = Set(next_reset_at);
        u_active.updated_at = Set(now);
        u_active.update(&txn).await?;

        let log = traffic_reset_log::ActiveModel {
            user_id: Set(u.id),
            reset_type: Set(format!(
                "{:?}",
                plan_reset_method.unwrap_or(system_reset_method)
            )),
            reset_time: Set(now),
            old_upload: Set(old_upload),
            old_download: Set(old_download),
            old_total: Set(old_total),
            new_upload: Set(0),
            new_download: Set(0),
            new_total: Set(0),
            trigger_source: Set("auto".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        log.insert(&txn).await?;

        txn.commit().await?;
        reset_count += 1;
    }

    Ok(reset_count)
}
