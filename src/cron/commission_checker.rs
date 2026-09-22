use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait};

use crate::{
    common::{AppError, AppState},
    entities::{commission_log, order, user, Order, User},
    services::order_service::STATUS_COMPLETED,
};

pub const COMMISSION_STATUS_PENDING: i32 = 0;
pub const COMMISSION_STATUS_VALID: i32 = 1;
pub const COMMISSION_STATUS_PAID: i32 = 2;

/// Checks pending commissions and distributes commissions to inviter(s).
/// Exactly matches PHP `CheckCommission` logic.
/// Returns the number of commissions successfully distributed.
pub async fn check_and_pay_commissions(state: &AppState) -> Result<usize, AppError> {
    let now = chrono::Utc::now().timestamp();

    // 1. Auto-check: If commission_auto_check_enable == 1 (default true),
    // orders completed for >= 3 days move from commission_status 0 to 1
    let auto_check_enable = state
        .setting_service
        .get_bool("commission_auto_check_enable", true)
        .await;

    if auto_check_enable {
        let three_days_ago = now - 3 * 86400;
        let pending_orders = Order::find()
            .filter(order::Column::CommissionStatus.eq(COMMISSION_STATUS_PENDING))
            .filter(order::Column::InviteUserId.is_not_null())
            .filter(order::Column::Status.eq(STATUS_COMPLETED))
            .filter(order::Column::UpdatedAt.lte(three_days_ago))
            .all(&state.db)
            .await?;

        for o in pending_orders {
            let mut o_active: order::ActiveModel = o.into();
            o_active.commission_status = Set(COMMISSION_STATUS_VALID);
            o_active.updated_at = Set(now);
            let _ = o_active.update(&state.db).await;
        }
    }

    // 2. Pay commission for orders with commission_status == 1
    let valid_orders = Order::find()
        .filter(order::Column::CommissionStatus.eq(COMMISSION_STATUS_VALID))
        .filter(order::Column::InviteUserId.is_not_null())
        .all(&state.db)
        .await?;

    let multi_tier_enable = state
        .setting_service
        .get_bool("commission_distribution_enable", false)
        .await;
    let l1 = state
        .setting_service
        .get_int("commission_distribution_l1", 100)
        .await;
    let l2 = state
        .setting_service
        .get_int("commission_distribution_l2", 0)
        .await;
    let l3 = state
        .setting_service
        .get_int("commission_distribution_l3", 0)
        .await;
    let withdraw_close = state
        .setting_service
        .get_bool("withdraw_close_enable", false)
        .await;

    let share_levels = if multi_tier_enable {
        vec![l1, l2, l3]
    } else {
        vec![100]
    };

    let mut paid_count = 0;

    for o in valid_orders {
        let mut curr_invite_user_id = o.invite_user_id;
        let base_commission = o.commission_balance as f64;
        let mut actual_paid_commission = 0;

        let txn = state.db.begin().await?;

        for &share_pct in &share_levels {
            if share_pct <= 0 {
                continue;
            }
            let inviter_id = match curr_invite_user_id {
                Some(id) => id,
                None => break,
            };

            let inviter = User::find_by_id(inviter_id).one(&txn).await?;
            if let Some(inv) = inviter {
                let share_amount =
                    ((base_commission * (share_pct as f64) / 100.0).round() as i32).max(0);
                if share_amount > 0 {
                    let mut inv_active: user::ActiveModel = inv.clone().into();
                    if withdraw_close {
                        inv_active.balance = Set(inv.balance + share_amount);
                    } else {
                        inv_active.commission_balance = Set(inv.commission_balance + share_amount);
                    }
                    inv_active.updated_at = Set(now);
                    inv_active.update(&txn).await?;

                    let log = commission_log::ActiveModel {
                        invite_user_id: Set(inviter_id),
                        user_id: Set(o.user_id),
                        trade_no: Set(o.trade_no.clone()),
                        order_amount: Set(o.total_amount),
                        get_amount: Set(share_amount),
                        created_at: Set(now),
                        updated_at: Set(now),
                        ..Default::default()
                    };
                    log.insert(&txn).await?;

                    actual_paid_commission += share_amount;
                }
                curr_invite_user_id = inv.invite_user_id;
            } else {
                break;
            }
        }

        let mut o_active: order::ActiveModel = o.into();
        o_active.commission_status = Set(COMMISSION_STATUS_PAID);
        o_active.actual_commission_balance = Set(Some(actual_paid_commission));
        o_active.updated_at = Set(now);
        o_active.update(&txn).await?;

        txn.commit().await?;
        paid_count += 1;
    }

    Ok(paid_count)
}
