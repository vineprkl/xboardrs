use crate::{
    common::AppError,
    entities::{order, user, Order, Payment, Plan, User},
    services::{CouponService, PlanService, SettingService},
};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STATUS_PENDING: i32 = 0;
pub const STATUS_PROCESSING: i32 = 1;
pub const STATUS_CANCELLED: i32 = 2;
pub const STATUS_COMPLETED: i32 = 3;
pub const STATUS_DISCOUNTED: i32 = 4;

pub const TYPE_NEW_PURCHASE: i32 = 1;
pub const TYPE_RENEWAL: i32 = 2;
pub const TYPE_UPGRADE: i32 = 3;
pub const TYPE_RESET_TRAFFIC: i32 = 4;

pub const COMMISSION_STATUS_PENDING: i32 = 0;
pub const COMMISSION_STATUS_COMPLETED: i32 = 1;
pub const COMMISSION_STATUS_CANCELLED: i32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDto {
    pub id: i32,
    pub invite_user_id: Option<i32>,
    pub user_id: i32,
    pub plan_id: i32,
    pub coupon_id: Option<i32>,
    pub payment_id: Option<i32>,
    #[serde(rename = "type")]
    pub r#type: i32,
    pub period: String,
    pub trade_no: String,
    pub callback_no: Option<String>,
    pub total_amount: i32,
    pub handling_amount: Option<i32>,
    pub discount_amount: Option<i32>,
    pub surplus_amount: Option<i32>,
    pub refund_amount: Option<i32>,
    pub balance_amount: Option<i32>,
    pub surplus_credit: Option<i32>,
    pub surplus_order_ids: Option<Vec<i32>>,
    pub status: i32,
    pub commission_status: i32,
    pub commission_balance: i32,
    pub actual_commission_balance: Option<i32>,
    pub paid_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub plan: Option<Value>,
    pub payment: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub try_out_plan_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surplus_orders: Option<Vec<Value>>,
}

#[derive(Clone)]
pub struct OrderService {
    db: DatabaseConnection,
    plan_service: PlanService,
    coupon_service: CouponService,
    setting_service: SettingService,
}

impl OrderService {
    pub fn new(
        db: DatabaseConnection,
        plan_service: PlanService,
        coupon_service: CouponService,
        setting_service: SettingService,
    ) -> Self {
        Self {
            db,
            plan_service,
            coupon_service,
            setting_service,
        }
    }

    /// Creates an order from user request matching PHP OrderService::createFromRequest().
    pub async fn create_from_request(
        &self,
        user_id: i32,
        plan_id: i32,
        period: &str,
        coupon_code: Option<&str>,
    ) -> Result<order::Model, AppError> {
        // 1. Check if user has an incomplete (pending or processing) order
        let incomplete = Order::find()
            .filter(order::Column::UserId.eq(user_id))
            .filter(order::Column::Status.is_in([STATUS_PENDING, STATUS_PROCESSING]))
            .one(&self.db)
            .await?;

        if incomplete.is_some() {
            return Err(AppError::Custom(
                400,
                "您有未完成的订单，请稍后再试或先取消".to_string(),
            ));
        }

        let user = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::Custom(400, "用户不存在".to_string()))?;

        let plan = Plan::find_by_id(plan_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::Custom(400, "订阅不存在".to_string()))?;

        // Validate purchase
        self.plan_service
            .validate_purchase(&user, &plan, period)
            .await?;

        let period_key = PlanService::get_period_key(period);
        let base_price = plan.get_price(&period_key).ok_or_else(|| {
            AppError::Custom(400, "该付款周期无法购买，请选择其他周期".to_string())
        })?;

        let mut total_amount = base_price;
        let mut discount_amount = 0;
        let mut coupon_id = None;

        // 2. Apply coupon if provided
        if let Some(code) = coupon_code {
            if !code.trim().is_empty() {
                let coupon_result = self
                    .coupon_service
                    .check_coupon(code, user.id, Some(plan.id), Some(&period_key))
                    .await?;
                let coupon_discount = match coupon_result.r#type {
                    1 => ((total_amount as f64) * (coupon_result.value as f64 / 100.0)).round()
                        as i32,
                    2 => coupon_result.value,
                    _ => 0,
                };
                discount_amount += coupon_discount;
                total_amount = (total_amount - coupon_discount).max(0);
                coupon_id = Some(coupon_result.id);
            }
        }

        // 3. Apply VIP discount if present
        if let Some(vip_discount) = user.discount {
            if vip_discount > 0 {
                let disc = ((total_amount as f64) * (vip_discount as f64 / 100.0)).round() as i32;
                discount_amount += disc;
                total_amount = (total_amount - disc).max(0);
            }
        }

        let now = Utc::now().timestamp();
        let order_type;
        let mut surplus_amount = 0;
        let mut surplus_credit = 0;
        let mut surplus_order_ids: Vec<i32> = Vec::new();

        // 4. Determine order type & calculate surplus
        if period_key == "reset_traffic" {
            order_type = TYPE_RESET_TRAFFIC;
        } else if user.plan_id.is_some()
            && user.plan_id != Some(plan.id)
            && (user.expired_at.is_none() || user.expired_at.unwrap() > now)
        {
            let plan_change_enable = self
                .setting_service
                .get_bool("plan_change_enable", true)
                .await;
            if !plan_change_enable {
                return Err(AppError::Custom(
                    400,
                    "目前不允许更改订阅，请联系客服或提交工单操作".to_string(),
                ));
            }
            order_type = TYPE_UPGRADE;
            let surplus_enable = self.setting_service.get_bool("surplus_enable", true).await;
            if surplus_enable {
                let (calc_surplus, calc_ids) =
                    Self::calculate_surplus_value(&self.db, &user, &self.setting_service, now)
                        .await?;
                surplus_amount = calc_surplus;
                surplus_order_ids = calc_ids;
            }

            if surplus_amount >= total_amount {
                surplus_credit = surplus_amount - total_amount;
                total_amount = 0;
            } else {
                total_amount -= surplus_amount;
            }
        } else if (user.expired_at.is_none() || user.expired_at.unwrap() > now)
            && user.plan_id == Some(plan.id)
        {
            order_type = TYPE_RENEWAL;
        } else {
            order_type = TYPE_NEW_PURCHASE;
        }

        // 5. Calculate invite commission
        let mut commission_balance = 0;
        let mut invite_user_id = None;
        if let Some(inv_id) = user.invite_user_id {
            if total_amount > 0 {
                invite_user_id = Some(inv_id);
                if let Some(inviter) = User::find_by_id(inv_id).one(&self.db).await? {
                    let mut commission_type = inviter.commission_type;
                    if commission_type == 0 {
                        let first_time = self
                            .setting_service
                            .get_bool("commission_first_time_enable", true)
                            .await;
                        commission_type = if first_time { 2 } else { 1 };
                    }
                    let is_commission = match commission_type {
                        1 => true,
                        2 => {
                            let prev_order = Order::find()
                                .filter(order::Column::UserId.eq(user.id))
                                .filter(order::Column::Status.is_in([
                                    STATUS_PROCESSING,
                                    STATUS_COMPLETED,
                                    STATUS_DISCOUNTED,
                                ]))
                                .one(&self.db)
                                .await?;
                            prev_order.is_none()
                        }
                        _ => false,
                    };
                    if is_commission {
                        let rate = if let Some(r) = inviter.commission_rate {
                            r
                        } else {
                            self.setting_service
                                .get_float("invite_commission", 10.0)
                                .await
                        };
                        commission_balance =
                            ((total_amount as f64) * (rate / 100.0)).round() as i32;
                    }
                }
            }
        }

        // 6. Begin transaction for atomic balance deduction and order insertion
        let txn = self.db.begin().await?;

        let mut balance_amount = 0;
        if user.balance > 0 && total_amount > 0 {
            if user.balance >= total_amount {
                balance_amount = total_amount;
                let mut u_active: user::ActiveModel = user.clone().into();
                u_active.balance = Set(user.balance - total_amount);
                u_active.update(&txn).await?;
                total_amount = 0;
            } else {
                balance_amount = user.balance;
                total_amount -= user.balance;
                let mut u_active: user::ActiveModel = user.clone().into();
                u_active.balance = Set(0);
                u_active.update(&txn).await?;
            }
        }

        let trade_no = crate::utils::generate_order_no();
        let surplus_ids_json = if surplus_order_ids.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&surplus_order_ids).unwrap_or_default())
        };

        let new_order = order::ActiveModel {
            user_id: Set(user.id),
            plan_id: Set(plan.id),
            coupon_id: Set(coupon_id),
            payment_id: Set(None),
            r#type: Set(order_type),
            period: Set(period_key),
            trade_no: Set(trade_no),
            callback_no: Set(None),
            total_amount: Set(total_amount),
            handling_amount: Set(None),
            discount_amount: Set(if discount_amount > 0 {
                Some(discount_amount)
            } else {
                None
            }),
            surplus_amount: Set(if surplus_amount > 0 {
                Some(surplus_amount)
            } else {
                None
            }),
            refund_amount: Set(None),
            balance_amount: Set(if balance_amount > 0 {
                Some(balance_amount)
            } else {
                None
            }),
            surplus_credit: Set(if surplus_credit > 0 {
                Some(surplus_credit)
            } else {
                None
            }),
            surplus_order_ids: Set(surplus_ids_json),
            status: Set(STATUS_PENDING),
            commission_status: Set(COMMISSION_STATUS_PENDING),
            commission_balance: Set(commission_balance),
            actual_commission_balance: Set(None),
            commission_rate: Set(None),
            commission_auto_check: Set(None),
            paid_at: Set(None),
            invite_user_id: Set(invite_user_id),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        let saved = new_order.insert(&txn).await?;
        txn.commit().await?;

        Ok(saved)
    }

    /// Marks an order as paid and opens/activates the subscription matching PHP OrderService::paid() and open().
    pub async fn paid(&self, trade_no: &str, callback_no: &str) -> Result<bool, AppError> {
        let txn = self.db.begin().await?;
        let order_opt = Order::find()
            .filter(order::Column::TradeNo.eq(trade_no))
            .one(&txn)
            .await?;

        let order = match order_opt {
            Some(o) => o,
            None => return Err(AppError::Custom(400, "Order not found".to_string())),
        };

        if order.status != STATUS_PENDING {
            return Ok(true); // Already paid or processed
        }

        let now = Utc::now().timestamp();
        let mut order_active: order::ActiveModel = order.clone().into();
        order_active.status = Set(STATUS_PROCESSING);
        order_active.paid_at = Set(Some(now));
        order_active.callback_no = Set(Some(callback_no.to_string()));
        order_active.updated_at = Set(now);
        let processing_order = order_active.update(&txn).await?;

        // Activate / open the order
        Self::open_order(&txn, &processing_order, &self.setting_service).await?;

        txn.commit().await?;
        Ok(true)
    }

    /// Activates subscription details, updates user traffic/quotas, and completes the order.
    pub async fn open_order<C: ConnectionTrait>(
        db: &C,
        order: &order::Model,
        setting_service: &SettingService,
    ) -> Result<(), AppError> {
        let now = Utc::now().timestamp();
        let user_opt = User::find_by_id(order.user_id).one(db).await?;
        let mut user = match user_opt {
            Some(u) => u,
            None => return Err(AppError::Custom(400, "User not found".to_string())),
        };

        let plan_opt = Plan::find_by_id(order.plan_id).one(db).await?;
        let plan = match plan_opt {
            Some(p) => p,
            None => return Err(AppError::Custom(400, "Plan not found".to_string())),
        };

        let mut user_active: user::ActiveModel = user.clone().into();

        // 1. Surplus credit returned to balance
        if let Some(surplus_credit) = order.surplus_credit {
            if surplus_credit > 0 {
                user.balance += surplus_credit;
                user_active.balance = Set(user.balance);
            }
        }

        // 2. Mark surplus orders as discounted
        if let Some(ref ids_str) = order.surplus_order_ids {
            if let Ok(ids) = serde_json::from_str::<Vec<i32>>(ids_str) {
                for id in ids {
                    if let Some(so) = Order::find_by_id(id).one(db).await? {
                        let mut so_active: order::ActiveModel = so.into();
                        so_active.status = Set(STATUS_DISCOUNTED);
                        so_active.updated_at = Set(now);
                        so_active.update(db).await?;
                    }
                }
            }
        }

        // 3. Process period activation
        let period_key = PlanService::get_period_key(&order.period);
        if period_key == "onetime" {
            user_active.u = Set(0);
            user_active.d = Set(0);
            user_active.transfer_enable = Set(plan.transfer_enable * 1_073_741_824);
            user_active.plan_id = Set(Some(plan.id));
            user_active.group_id = Set(Some(plan.group_id));
            user_active.expired_at = Set(None);
        } else if period_key == "reset_traffic" {
            user_active.u = Set(0);
            user_active.d = Set(0);
        } else {
            let mut base_expired = user.expired_at.unwrap_or(0);
            if order.r#type == TYPE_UPGRADE {
                base_expired = now;
            }
            user_active.transfer_enable = Set(plan.transfer_enable * 1_073_741_824);
            if user.expired_at.is_none() || order.r#type == TYPE_NEW_PURCHASE {
                user_active.u = Set(0);
                user_active.d = Set(0);
            }
            user_active.plan_id = Set(Some(plan.id));
            user_active.group_id = Set(Some(plan.group_id));

            let months = PlanService::get_period_months(&period_key).unwrap_or(1);
            let start_time = if base_expired < now {
                now
            } else {
                base_expired
            };
            let new_expired = add_months_to_timestamp(start_time, months);
            user_active.expired_at = Set(Some(new_expired));
        }

        user_active.speed_limit = Set(plan.speed_limit);
        user_active.device_limit = Set(plan.device_limit);
        user_active.updated_at = Set(now);

        // Open event handling
        let event_key = match order.r#type {
            TYPE_NEW_PURCHASE => "new_order_event_id",
            TYPE_RENEWAL => "renew_order_event_id",
            TYPE_UPGRADE => "change_order_event_id",
            _ => "",
        };
        if !event_key.is_empty() && setting_service.get_int(event_key, 0).await == 1 {
            user_active.u = Set(0);
            user_active.d = Set(0);
        }

        user_active.update(db).await?;

        // 4. Mark order as COMPLETED
        let mut o_active: order::ActiveModel = order.clone().into();
        o_active.status = Set(STATUS_COMPLETED);
        o_active.updated_at = Set(now);
        o_active.update(db).await?;

        Ok(())
    }

    /// Cancels a pending order and refunds any deducted account balance.
    pub async fn cancel(&self, trade_no: &str, user_id: i32) -> Result<bool, AppError> {
        let txn = self.db.begin().await?;
        let order_opt = Order::find()
            .filter(order::Column::TradeNo.eq(trade_no))
            .filter(order::Column::UserId.eq(user_id))
            .one(&txn)
            .await?;

        let order = match order_opt {
            Some(o) => o,
            None => return Err(AppError::Custom(400, "Order does not exist".to_string())),
        };

        if order.status != STATUS_PENDING {
            return Err(AppError::Custom(
                400,
                "You can only cancel pending orders".to_string(),
            ));
        }

        let now = Utc::now().timestamp();
        // Refund balance if balance_amount > 0
        if let Some(balance_amount) = order.balance_amount {
            if balance_amount > 0 {
                let user_opt = User::find_by_id(order.user_id).one(&txn).await?;
                if let Some(user) = user_opt {
                    let mut u_active: user::ActiveModel = user.clone().into();
                    u_active.balance = Set(user.balance + balance_amount);
                    u_active.updated_at = Set(now);
                    u_active.update(&txn).await?;
                }
            }
        }

        let mut o_active: order::ActiveModel = order.into();
        o_active.status = Set(STATUS_CANCELLED);
        o_active.updated_at = Set(now);
        o_active.update(&txn).await?;

        txn.commit().await?;
        Ok(true)
    }

    /// Calculates surplus value for upgrade orders matching PHP OrderService::getSurplusValue().
    pub async fn calculate_surplus_value(
        db: &DatabaseConnection,
        user: &user::Model,
        setting_service: &SettingService,
        now: i64,
    ) -> Result<(i32, Vec<i32>), AppError> {
        if user.expired_at.is_none() {
            // One-time plan surplus
            let last_onetime = Order::find()
                .filter(order::Column::UserId.eq(user.id))
                .filter(order::Column::Period.eq("onetime"))
                .filter(order::Column::Status.eq(STATUS_COMPLETED))
                .order_by_desc(order::Column::Id)
                .one(db)
                .await?;

            if let Some(last_order) = last_onetime {
                let now_user_traffic_gb = user.transfer_enable as f64 / 1_073_741_824.0;
                if now_user_traffic_gb <= 0.0 {
                    return Ok((0, vec![]));
                }
                let paid_total =
                    (last_order.total_amount + last_order.balance_amount.unwrap_or(0)) as f64;
                if paid_total <= 0.0 {
                    return Ok((0, vec![]));
                }
                let traffic_unit_price = paid_total / now_user_traffic_gb;
                let used_traffic_gb = (user.u + user.d) as f64 / 1_073_741_824.0;
                let not_used_traffic = (now_user_traffic_gb - used_traffic_gb).max(0.0);
                let result = (traffic_unit_price * not_used_traffic).round() as i32;

                let completed_orders = Order::find()
                    .filter(order::Column::UserId.eq(user.id))
                    .filter(order::Column::Period.ne("reset_traffic"))
                    .filter(order::Column::Status.eq(STATUS_COMPLETED))
                    .all(db)
                    .await?;
                let ids = completed_orders.into_iter().map(|o| o.id).collect();
                return Ok((result.max(0), ids));
            }
            Ok((0, vec![]))
        } else {
            // Period plan surplus
            let completed_orders = Order::find()
                .filter(order::Column::UserId.eq(user.id))
                .filter(order::Column::Period.ne("reset_traffic"))
                .filter(order::Column::Period.ne("onetime"))
                .filter(order::Column::Status.eq(STATUS_COMPLETED))
                .all(db)
                .await?;

            if completed_orders.is_empty() {
                return Ok((0, vec![]));
            }

            let mut order_amount_sum: i64 = 0;
            let mut order_month_sum: i64 = 0;
            let mut first_order_at = i64::MAX;
            let ids: Vec<i32> = completed_orders.iter().map(|o| o.id).collect();

            for o in &completed_orders {
                let net_amount =
                    (o.total_amount + o.balance_amount.unwrap_or(0) + o.surplus_amount.unwrap_or(0)
                        - o.surplus_credit.unwrap_or(0)) as i64;
                order_amount_sum += net_amount;
                let months = PlanService::get_period_months(&o.period).unwrap_or(0);
                order_month_sum += months;
                if o.created_at < first_order_at {
                    first_order_at = o.created_at;
                }
            }

            if first_order_at == i64::MAX {
                first_order_at = now;
            }

            let expired_at = add_months_to_timestamp(first_order_at, order_month_sum);
            let total_seconds = (expired_at - first_order_at).max(0);
            let remain_seconds = (expired_at - now).max(0);
            let cycle_ratio = if total_seconds > 0 {
                remain_seconds as f64 / total_seconds as f64
            } else {
                0.0
            };

            let mut ratio = cycle_ratio;
            let event_id = setting_service.get_int("change_order_event_id", 0).await;
            if event_id == 1 {
                if let Some(plan_id) = user.plan_id {
                    if let Some(user_plan) = Plan::find_by_id(plan_id).one(db).await? {
                        let total_traffic = (user_plan.transfer_enable as f64 / 1_073_741_824.0)
                            * (order_month_sum as f64);
                        let used_traffic = (user.u + user.d) as f64 / 1_073_741_824.0;
                        let remain_traffic = (total_traffic - used_traffic).max(0.0);
                        let traffic_ratio = if total_traffic > 0.0 {
                            remain_traffic / total_traffic
                        } else {
                            0.0
                        };
                        ratio = cycle_ratio.min(traffic_ratio);
                    }
                }
            }

            let surplus_amount = ((order_amount_sum as f64) * ratio).round() as i32;
            Ok((surplus_amount.max(0), ids))
        }
    }

    /// Formats an order model into OrderDto matching Laravel OrderResource.
    pub async fn format_order(
        &self,
        order: &order::Model,
        with_detail: bool,
    ) -> Result<OrderDto, AppError> {
        let plan_val = Plan::find_by_id(order.plan_id)
            .one(&self.db)
            .await?
            .map(|p| {
                serde_json::to_value(self.plan_service.format_plan(&p)).unwrap_or(Value::Null)
            });

        let payment_val = if let Some(pay_id) = order.payment_id {
            Payment::find_by_id(pay_id).one(&self.db).await?.map(|pm| {
                serde_json::json!({
                    "id": pm.id,
                    "name": pm.name,
                    "payment": pm.payment,
                    "icon": pm.icon
                })
            })
        } else {
            None
        };

        let surplus_ids: Option<Vec<i32>> = order
            .surplus_order_ids
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        let (try_out_id, surplus_orders) = if with_detail {
            let to_val = self.setting_service.get_int("try_out_plan_id", 0).await;
            let to_id = if to_val > 0 {
                Some(to_val as i32)
            } else {
                None
            };
            let s_orders = if let Some(ref ids) = surplus_ids {
                if !ids.is_empty() {
                    let orders = Order::find()
                        .filter(order::Column::Id.is_in(ids.clone()))
                        .all(&self.db)
                        .await?;
                    Some(
                        orders
                            .into_iter()
                            .map(|o| serde_json::to_value(o).unwrap_or(Value::Null))
                            .collect(),
                    )
                } else {
                    None
                }
            } else {
                None
            };
            (to_id, s_orders)
        } else {
            (None, None)
        };

        Ok(OrderDto {
            id: order.id,
            invite_user_id: order.invite_user_id,
            user_id: order.user_id,
            plan_id: order.plan_id,
            coupon_id: order.coupon_id,
            payment_id: order.payment_id,
            r#type: order.r#type,
            period: PlanService::get_legacy_period(&order.period),
            trade_no: order.trade_no.clone(),
            callback_no: order.callback_no.clone(),
            total_amount: order.total_amount,
            handling_amount: order.handling_amount,
            discount_amount: order.discount_amount,
            surplus_amount: order.surplus_amount,
            refund_amount: order.refund_amount,
            balance_amount: order.balance_amount,
            surplus_credit: order.surplus_credit,
            surplus_order_ids: surplus_ids,
            status: order.status,
            commission_status: order.commission_status,
            commission_balance: order.commission_balance,
            actual_commission_balance: order.actual_commission_balance,
            paid_at: order.paid_at,
            created_at: order.created_at,
            updated_at: order.updated_at,
            plan: plan_val,
            payment: payment_val,
            try_out_plan_id: try_out_id,
            surplus_orders,
        })
    }
}

pub fn add_months_to_timestamp(ts: i64, months: i64) -> i64 {
    let dt = DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now);
    let mut year = dt.year();
    let mut month = dt.month() as i64 + months;
    while month > 12 {
        year += 1;
        month -= 12;
    }
    while month < 1 {
        year -= 1;
        month += 12;
    }
    let day = dt.day();
    let d = NaiveDate::from_ymd_opt(year, month as u32, day)
        .or_else(|| NaiveDate::from_ymd_opt(year, month as u32, 28))
        .unwrap_or_else(|| dt.date_naive());
    let new_dt = d.and_time(dt.time()).and_utc();
    new_dt.timestamp()
}
