use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{
        commission_log, order, server, ticket, user, CommissionLog, Order, Server, Ticket, User,
    },
    handlers::auth::AuthenticatedAdmin,
    services::order_service::{STATUS_CANCELLED, STATUS_PENDING},
};

/// GET /api/v2/admin/stat/getOverride
pub async fn get_override(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();
    let today_start = chrono::Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp();

    let month_start =
        chrono::NaiveDate::from_ymd_opt(chrono::Utc::now().year(), chrono::Utc::now().month(), 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();

    use chrono::Datelike;

    let orders_this_month = Order::find()
        .filter(order::Column::CreatedAt.gte(month_start))
        .filter(order::Column::CreatedAt.lte(now))
        .filter(order::Column::Status.is_not_in([STATUS_PENDING, STATUS_CANCELLED]))
        .all(&state.db)
        .await?;
    let month_income: i64 = orders_this_month
        .iter()
        .map(|o| o.total_amount as i64)
        .sum();

    let orders_today = Order::find()
        .filter(order::Column::CreatedAt.gte(today_start))
        .filter(order::Column::CreatedAt.lte(now))
        .filter(order::Column::Status.is_not_in([STATUS_PENDING, STATUS_CANCELLED]))
        .all(&state.db)
        .await?;
    let day_income: i64 = orders_today.iter().map(|o| o.total_amount as i64).sum();

    let month_register_total = User::find()
        .filter(user::Column::CreatedAt.gte(month_start))
        .filter(user::Column::CreatedAt.lte(now))
        .count(&state.db)
        .await?;

    let ticket_pending_total = Ticket::find()
        .filter(ticket::Column::Status.eq(0))
        .count(&state.db)
        .await?;

    let commission_pending_total = Order::find()
        .filter(order::Column::CommissionStatus.eq(0))
        .filter(order::Column::InviteUserId.is_not_null())
        .filter(order::Column::Status.is_not_in([STATUS_PENDING, STATUS_CANCELLED]))
        .filter(order::Column::CommissionBalance.gt(0))
        .count(&state.db)
        .await?;

    let commission_month_payout: i64 = CommissionLog::find()
        .filter(commission_log::Column::CreatedAt.gte(month_start))
        .filter(commission_log::Column::CreatedAt.lte(now))
        .all(&state.db)
        .await?
        .iter()
        .map(|c| c.get_amount as i64)
        .sum();

    let servers = Server::find().all(&state.db).await?;
    let mut online_nodes = 0;
    for s in &servers {
        if state.server_service.is_node_online(s.id).await {
            online_nodes += 1;
        }
    }

    let online_users = User::find()
        .filter(user::Column::T.gte(now - 600))
        .count(&state.db)
        .await?;

    let all_users = User::find().all(&state.db).await?;
    let total_upload: i64 = all_users.iter().map(|u| u.u).sum();
    let total_download: i64 = all_users.iter().map(|u| u.d).sum();

    let data = json!({
        "month_income": month_income,
        "day_income": day_income,
        "last_month_income": 0,
        "month_register_total": month_register_total,
        "ticket_pending_total": ticket_pending_total,
        "commission_pending_total": commission_pending_total,
        "commission_month_payout": commission_month_payout,
        "commission_last_month_payout": 0,
        "online_nodes": online_nodes,
        "online_devices": online_users,
        "online_users": online_users,
        "today_traffic": {
            "upload": 0,
            "download": 0,
            "total": 0
        },
        "month_traffic": {
            "upload": total_upload,
            "download": total_download,
            "total": total_upload + total_download
        },
        "total_traffic": {
            "upload": total_upload,
            "download": total_download,
            "total": total_upload + total_download
        }
    });

    Ok(ApiResponse::success(data).into_response())
}

#[derive(Debug, Deserialize, Default)]
pub struct GetOrderQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    #[serde(rename = "type")]
    pub stat_type: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TrafficRankQuery {
    #[serde(rename = "type")]
    pub rank_type: Option<String>,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
}

/// GET /api/v2/admin/stat/getStats
pub async fn get_stats(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    use chrono::Datelike;

    let now = chrono::Utc::now().timestamp();
    let today_start = chrono::Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp();
    let yesterday_start = today_start - 86400;

    let current_month_start =
        chrono::NaiveDate::from_ymd_opt(chrono::Utc::now().year(), chrono::Utc::now().month(), 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();

    // Today income
    let today_orders = Order::find()
        .filter(order::Column::CreatedAt.gte(today_start))
        .filter(order::Column::CreatedAt.lte(now))
        .filter(order::Column::Status.is_not_in([STATUS_PENDING, STATUS_CANCELLED]))
        .all(&state.db)
        .await?;
    let today_income: i64 = today_orders.iter().map(|o| o.total_amount as i64).sum();

    // Yesterday income
    let yesterday_orders = Order::find()
        .filter(order::Column::CreatedAt.gte(yesterday_start))
        .filter(order::Column::CreatedAt.lt(today_start))
        .filter(order::Column::Status.is_not_in([STATUS_PENDING, STATUS_CANCELLED]))
        .all(&state.db)
        .await?;
    let yesterday_income: i64 = yesterday_orders.iter().map(|o| o.total_amount as i64).sum();
    let day_income_growth = if yesterday_income > 0 {
        ((today_income - yesterday_income) as f64 / yesterday_income as f64 * 100.0 * 10.0).round()
            / 10.0
    } else {
        0.0
    };

    // Month income
    let month_orders = Order::find()
        .filter(order::Column::CreatedAt.gte(current_month_start))
        .filter(order::Column::CreatedAt.lte(now))
        .filter(order::Column::Status.is_not_in([STATUS_PENDING, STATUS_CANCELLED]))
        .all(&state.db)
        .await?;
    let current_month_income: i64 = month_orders.iter().map(|o| o.total_amount as i64).sum();

    // Commission
    let current_month_commission_payout: i64 = CommissionLog::find()
        .filter(commission_log::Column::CreatedAt.gte(current_month_start))
        .filter(commission_log::Column::CreatedAt.lte(now))
        .all(&state.db)
        .await?
        .iter()
        .map(|c| c.get_amount as i64)
        .sum();

    let commission_pending_total = Order::find()
        .filter(order::Column::CommissionStatus.eq(0))
        .filter(order::Column::InviteUserId.is_not_null())
        .filter(order::Column::Status.eq(crate::services::order_service::STATUS_COMPLETED))
        .filter(order::Column::CommissionBalance.gt(0))
        .count(&state.db)
        .await?;

    // User counts
    let current_month_new_users = User::find()
        .filter(user::Column::CreatedAt.gte(current_month_start))
        .filter(user::Column::CreatedAt.lte(now))
        .count(&state.db)
        .await?;

    let total_users = User::find().count(&state.db).await?;
    let active_users = User::find()
        .filter(
            user::Column::ExpiredAt
                .gte(now)
                .or(user::Column::ExpiredAt.is_null()),
        )
        .count(&state.db)
        .await?;

    let online_users = User::find()
        .filter(user::Column::T.gte(now - 600))
        .count(&state.db)
        .await?;

    let ticket_pending_total = Ticket::find()
        .filter(ticket::Column::Status.eq(0))
        .count(&state.db)
        .await?;

    let servers = Server::find().all(&state.db).await?;
    let mut online_nodes = 0;
    for s in &servers {
        if state.server_service.is_node_online(s.id).await {
            online_nodes += 1;
        }
    }

    let all_users = User::find().all(&state.db).await?;
    let total_upload: i64 = all_users.iter().map(|u| u.u).sum();
    let total_download: i64 = all_users.iter().map(|u| u.d).sum();

    let data = json!({
        "todayIncome": today_income,
        "dayIncomeGrowth": day_income_growth,
        "currentMonthIncome": current_month_income,
        "lastMonthIncome": 0,
        "monthIncomeGrowth": 0,
        "lastMonthIncomeGrowth": 0,
        "currentMonthCommissionPayout": current_month_commission_payout,
        "lastMonthCommissionPayout": 0,
        "commissionGrowth": 0,
        "commissionPendingTotal": commission_pending_total,
        "currentMonthNewUsers": current_month_new_users,
        "totalUsers": total_users,
        "activeUsers": active_users,
        "userGrowth": 0,
        "onlineUsers": online_users,
        "onlineDevices": online_users,
        "ticketPendingTotal": ticket_pending_total,
        "onlineNodes": online_nodes,
        "todayTraffic": {
            "upload": 0,
            "download": 0,
            "total": 0
        },
        "monthTraffic": {
            "upload": total_upload,
            "download": total_download,
            "total": total_upload + total_download
        },
        "totalTraffic": {
            "upload": total_upload,
            "download": total_download,
            "total": total_upload + total_download
        }
    });

    Ok(ApiResponse::success(data).into_response())
}

/// GET /api/v2/admin/stat/getRanking
pub async fn get_ranking(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let users = User::find()
        .order_by_desc(user::Column::U)
        .limit(10)
        .all(&state.db)
        .await?;

    let mut list = Vec::new();
    for u in users {
        list.push(json!({
            "id": u.id,
            "email": u.email,
            "u": u.u,
            "d": u.d,
            "total": u.u + u.d,
        }));
    }

    Ok(ApiResponse::success(list).into_response())
}

/// GET /api/v2/admin/stat/getServerLastRank
pub async fn get_server_last_rank(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let servers = Server::find()
        .order_by_desc(server::Column::U)
        .limit(10)
        .all(&state.db)
        .await?;

    Ok(ApiResponse::success(servers).into_response())
}

/// GET /api/v2/admin/stat/getServerYesterdayRank
pub async fn get_server_yesterday_rank(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let servers = Server::find()
        .order_by_desc(server::Column::D)
        .limit(10)
        .all(&state.db)
        .await?;

    Ok(ApiResponse::success(servers).into_response())
}

/// GET /api/v2/admin/stat/getTrafficRank
pub async fn get_traffic_rank(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<TrafficRankQuery>,
) -> Result<Response, AppError> {
    let rank_type = query.rank_type.as_deref().unwrap_or("node");
    let now_str = chrono::Utc::now().to_rfc3339();

    if rank_type == "user" {
        let users = User::find()
            .order_by_desc(user::Column::U)
            .limit(10)
            .all(&state.db)
            .await?;

        let mut result = Vec::new();
        for u in users {
            result.push(json!({
                "id": u.id.to_string(),
                "name": u.email,
                "value": u.u + u.d,
                "previousValue": 0,
                "change": 0.0,
                "timestamp": now_str,
            }));
        }
        return Ok(ApiResponse::success(result).into_response());
    }

    let servers = Server::find()
        .order_by_desc(server::Column::U)
        .limit(10)
        .all(&state.db)
        .await?;

    let mut result = Vec::new();
    for s in servers {
        result.push(json!({
            "id": s.id.to_string(),
            "name": s.name,
            "value": s.u.unwrap_or(0) + s.d.unwrap_or(0),
            "previousValue": 0,
            "change": 0.0,
            "timestamp": now_str,
        }));
    }

    Ok(ApiResponse::success(result).into_response())
}

/// GET /api/v2/admin/stat/getOrder
pub async fn get_order(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<GetOrderQuery>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();
    let start_ts = query
        .start_date
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc().timestamp())
        .unwrap_or(now - 86400 * 30);

    let end_ts = query
        .end_date
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .and_then(|d| d.and_hms_opt(23, 59, 59))
        .map(|dt| dt.and_utc().timestamp())
        .unwrap_or(now);

    let orders = Order::find()
        .filter(order::Column::CreatedAt.gte(start_ts))
        .filter(order::Column::CreatedAt.lte(end_ts))
        .filter(order::Column::Status.is_not_in([STATUS_PENDING, STATUS_CANCELLED]))
        .all(&state.db)
        .await?;

    let mut daily_map: std::collections::BTreeMap<String, (i64, i64, i64, i64)> =
        std::collections::BTreeMap::new();
    let mut paid_total: i64 = 0;
    let paid_count = orders.len() as i64;
    let mut commission_total: i64 = 0;
    let mut commission_count: i64 = 0;

    for o in &orders {
        paid_total += o.total_amount as i64;
        let date_str = chrono::DateTime::from_timestamp(o.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let entry = daily_map.entry(date_str).or_insert((0, 0, 0, 0));
        entry.0 += o.total_amount as i64;
        entry.1 += 1;
        if o.commission_balance > 0 {
            commission_total += o.commission_balance as i64;
            commission_count += 1;
            entry.2 += o.commission_balance as i64;
            entry.3 += 1;
        }
    }

    let mut daily_stats = Vec::new();
    for (date, (pt, pc, ct, cc)) in daily_map {
        let avg_order = if pc > 0 {
            (pt as f64 / pc as f64 * 100.0).round() / 100.0
        } else {
            0.0
        };
        let avg_comm = if cc > 0 {
            (ct as f64 / cc as f64 * 100.0).round() / 100.0
        } else {
            0.0
        };
        daily_stats.push(json!({
            "date": date,
            "paid_total": pt,
            "paid_count": pc,
            "commission_total": ct,
            "commission_count": cc,
            "avg_order_amount": avg_order,
            "avg_commission_amount": avg_comm,
        }));
    }

    let avg_paid = if paid_count > 0 {
        (paid_total as f64 / paid_count as f64 * 100.0).round() / 100.0
    } else {
        0.0
    };
    let avg_commission = if commission_count > 0 {
        (commission_total as f64 / commission_count as f64 * 100.0).round() / 100.0
    } else {
        0.0
    };
    let commission_rate = if paid_total > 0 {
        (commission_total as f64 / paid_total as f64 * 10000.0).round() / 100.0
    } else {
        0.0
    };

    let start_date_str = query.start_date.unwrap_or_else(|| {
        chrono::DateTime::from_timestamp(start_ts, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    });
    let end_date_str = query.end_date.unwrap_or_else(|| {
        chrono::DateTime::from_timestamp(end_ts, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    });

    let summary = json!({
        "paid_total": paid_total,
        "paid_count": paid_count,
        "commission_total": commission_total,
        "commission_count": commission_count,
        "start_date": start_date_str,
        "end_date": end_date_str,
        "avg_paid_amount": avg_paid,
        "avg_commission_amount": avg_commission,
        "commission_rate": commission_rate,
    });

    let res = json!({
        "list": daily_stats,
        "summary": summary,
    });

    Ok(ApiResponse::success(res).into_response())
}

/// GET or POST /api/v2/admin/stat/getStatUser
pub async fn get_stat_user(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let users = User::find().limit(50).all(&state.db).await?;
    Ok(ApiResponse::success(users).into_response())
}

/// GET /api/v2/admin/stat/getStatRecord
pub async fn get_stat_record(
    State(_state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    Ok(ApiResponse::success(json!([])).into_response())
}
