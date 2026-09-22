use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};

use crate::{
    common::{AppError, AppState},
    entities::{order, Order},
    services::{
        order_service::{STATUS_PENDING, STATUS_PROCESSING},
        OrderService,
    },
};

/// Scans and handles pending & processing orders.
/// - Cancels expired pending orders (default 120 minutes timeout).
/// - Completes processing orders.
///
/// Returns the number of expired orders cancelled.
pub async fn check_orders(state: &AppState) -> Result<usize, AppError> {
    let now = chrono::Utc::now().timestamp();
    let timeout_minutes = state.setting_service.get_int("order_timeout", 120).await;
    let timeout_seconds = (timeout_minutes.max(1)) * 60;
    let cutoff_time = now - timeout_seconds;

    // 1. Expired pending orders
    let pending_orders = Order::find()
        .filter(order::Column::Status.eq(STATUS_PENDING))
        .filter(order::Column::CreatedAt.lte(cutoff_time))
        .all(&state.db)
        .await?;

    let mut cancelled = 0;
    for o in pending_orders {
        if state
            .order_service
            .cancel(&o.trade_no, o.user_id)
            .await
            .is_ok()
        {
            cancelled += 1;
        }
    }

    // 2. Stuck processing orders
    let processing_orders = Order::find()
        .filter(order::Column::Status.eq(STATUS_PROCESSING))
        .all(&state.db)
        .await?;

    for o in processing_orders {
        if let Ok(txn) = state.db.begin().await {
            if OrderService::open_order(&txn, &o, &state.setting_service)
                .await
                .is_ok()
            {
                let _ = txn.commit().await;
            }
        }
    }

    Ok(cancelled)
}
