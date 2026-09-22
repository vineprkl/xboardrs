use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};
use serde::{Deserialize, Serialize};

use crate::{
    common::AppError,
    entities::{coupon, order, Coupon, Order},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouponCheckDto {
    pub code: String,
    pub plan_id: Option<i32>,
    pub period: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouponResource {
    pub id: i32,
    pub code: String,
    pub name: String,
    pub r#type: i32,
    pub value: i32,
    pub show: bool,
    pub limit_use: Option<i32>,
    pub limit_use_with_user: Option<i32>,
    pub limit_plan_ids: Option<Vec<i32>>,
    pub limit_period: Option<Vec<String>>,
    pub started_at: i64,
    pub ended_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct CouponService {
    db: DatabaseConnection,
}

impl CouponService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Validates and checks a coupon for a user with optional plan and period constraints.
    pub async fn check_coupon(
        &self,
        code: &str,
        user_id: i32,
        plan_id: Option<i32>,
        period: Option<&str>,
    ) -> Result<CouponResource, AppError> {
        let code = code.trim();
        if code.is_empty() {
            return Err(AppError::UnprocessableEntity(
                "Coupon cannot be empty".into(),
            ));
        }

        let coupon_model = Coupon::find()
            .filter(coupon::Column::Code.eq(code))
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::BadRequest("Invalid coupon".into()))?;

        if !coupon_model.show {
            return Err(AppError::BadRequest("Invalid coupon".into()));
        }

        if let Some(limit_use) = coupon_model.limit_use {
            if limit_use <= 0 {
                return Err(AppError::BadRequest(
                    "This coupon is no longer available".into(),
                ));
            }
        }

        let now = chrono::Utc::now().timestamp();
        if now < coupon_model.started_at {
            return Err(AppError::BadRequest(
                "This coupon has not yet started".into(),
            ));
        }

        if now > coupon_model.ended_at {
            return Err(AppError::BadRequest("This coupon has expired".into()));
        }

        let parsed_plan_ids: Option<Vec<i32>> = coupon_model
            .limit_plan_ids
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        if let (Some(limit_plans), Some(pid)) = (&parsed_plan_ids, plan_id) {
            if !limit_plans.is_empty() && !limit_plans.contains(&pid) {
                return Err(AppError::BadRequest(
                    "The coupon code cannot be used for this subscription".into(),
                ));
            }
        }

        let parsed_periods: Option<Vec<String>> = coupon_model
            .limit_period
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        if let (Some(limit_periods), Some(per)) = (&parsed_periods, period) {
            if !limit_periods.is_empty()
                && !limit_periods.iter().any(|p| p.eq_ignore_ascii_case(per))
            {
                return Err(AppError::BadRequest(
                    "The coupon code cannot be used for this period".into(),
                ));
            }
        }

        if let Some(limit_user) = coupon_model.limit_use_with_user {
            let used_count = Order::find()
                .filter(order::Column::CouponId.eq(coupon_model.id))
                .filter(order::Column::UserId.eq(user_id))
                .filter(order::Column::Status.is_not_in(vec![0, 2])) // not pending (0) and not cancelled (2)
                .count(&self.db)
                .await?;

            if used_count >= limit_user as u64 {
                return Err(AppError::BadRequest(format!(
                    "The coupon can only be used {} per person",
                    limit_user
                )));
            }
        }

        Ok(CouponResource {
            id: coupon_model.id,
            code: coupon_model.code,
            name: coupon_model.name,
            r#type: coupon_model.r#type,
            value: coupon_model.value,
            show: coupon_model.show,
            limit_use: coupon_model.limit_use,
            limit_use_with_user: coupon_model.limit_use_with_user,
            limit_plan_ids: parsed_plan_ids,
            limit_period: parsed_periods,
            started_at: coupon_model.started_at,
            ended_at: coupon_model.ended_at,
            created_at: coupon_model.created_at,
            updated_at: coupon_model.updated_at,
        })
    }

    /// Calculates discounted amount given a coupon and total amount in cents.
    pub fn calculate_discount(coupon_type: i32, coupon_value: i32, total_amount: i32) -> i32 {
        let discount = match coupon_type {
            1 => coupon_value, // Fixed discount in cents
            2 => ((total_amount as f64) * (coupon_value as f64 / 100.0)).round() as i32, // Percentage
            _ => 0,
        };
        discount.min(total_amount).max(0)
    }
}
