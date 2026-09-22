use crate::{
    common::AppError,
    entities::{plan, user, Plan, User},
};
use chrono::Utc;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanDto {
    pub id: i32,
    pub group_id: i32,
    pub name: String,
    pub tags: Option<Value>,
    pub content: Option<String>,
    pub month_price: Option<i32>,
    pub quarter_price: Option<i32>,
    pub half_year_price: Option<i32>,
    pub year_price: Option<i32>,
    pub two_year_price: Option<i32>,
    pub three_year_price: Option<i32>,
    pub onetime_price: Option<i32>,
    pub reset_price: Option<i32>,
    pub capacity_limit: Option<Value>,
    pub transfer_enable: i64,
    pub speed_limit: Option<i32>,
    pub device_limit: Option<i32>,
    pub show: bool,
    pub sell: bool,
    pub renew: bool,
    pub reset_traffic_method: Option<i32>,
    pub sort: Option<i32>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct PlanService {
    db: DatabaseConnection,
}

impl PlanService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Fetches all public available plans for sale matching PHP `PlanService::getAvailablePlans()`.
    pub async fn get_available_plans(&self) -> Result<Vec<PlanDto>, AppError> {
        let now = Utc::now().timestamp();
        let plans = Plan::find()
            .filter(plan::Column::Show.eq(true))
            .filter(plan::Column::Sell.is_null().or(plan::Column::Sell.eq(true)))
            .order_by_asc(plan::Column::Sort)
            .all(&self.db)
            .await?;

        let mut list = Vec::new();
        for p in plans {
            // Check capacity limit
            let capacity_val = if let Some(limit) = p.capacity_limit {
                let active_count = User::find()
                    .filter(user::Column::PlanId.eq(p.id))
                    .filter(
                        user::Column::ExpiredAt
                            .is_null()
                            .or(user::Column::ExpiredAt.gte(now)),
                    )
                    .all(&self.db)
                    .await?
                    .len() as i32;

                let remaining = limit - active_count;
                if remaining <= 0 {
                    continue; // In PHP PlanService::getAvailablePlans, sold out plans are filtered out
                }
                Value::Number(remaining.into())
            } else {
                Value::Null
            };

            list.push(self.format_plan_with_capacity(&p, capacity_val));
        }

        Ok(list)
    }

    /// Formats a single plan model into PlanDto with capacity set to null.
    pub fn format_plan(&self, p: &plan::Model) -> PlanDto {
        self.format_plan_with_capacity(p, Value::Null)
    }

    /// Formats a plan model into PlanDto with a specified capacity value.
    pub fn format_plan_with_capacity(&self, p: &plan::Model, capacity_val: Value) -> PlanDto {
        let tags_val = p
            .tags
            .as_deref()
            .and_then(|t| serde_json::from_str::<Value>(t).ok());
        let formatted_content = p.content.as_ref().map(|raw| {
            let speed = match p.speed_limit {
                Some(s) => s.to_string(),
                None => "No Limit".to_string(),
            };
            let devices = match p.device_limit {
                Some(d) => d.to_string(),
                None => "No Limit".to_string(),
            };
            let reset_method = match p.reset_traffic_method {
                Some(0) => "First Day of Month",
                Some(1) => "Monthly",
                Some(2) => "Never",
                Some(3) => "First Day of Year",
                Some(4) => "Yearly",
                _ => "Monthly",
            };
            raw.replace("{{transfer}}", &p.transfer_enable.to_string())
                .replace("{{speed}}", &speed)
                .replace("{{devices}}", &devices)
                .replace("{{reset_method}}", reset_method)
        });

        PlanDto {
            id: p.id,
            group_id: p.group_id,
            name: p.name.clone(),
            tags: tags_val,
            content: formatted_content,
            month_price: p.get_price("monthly"),
            quarter_price: p.get_price("quarterly"),
            half_year_price: p.get_price("half_yearly"),
            year_price: p.get_price("yearly"),
            two_year_price: p.get_price("two_yearly"),
            three_year_price: p.get_price("three_yearly"),
            onetime_price: p.get_price("onetime"),
            reset_price: p.get_price("reset_traffic"),
            capacity_limit: if capacity_val.is_null() {
                None
            } else {
                Some(capacity_val)
            },
            transfer_enable: p.transfer_enable,
            speed_limit: p.speed_limit,
            device_limit: p.device_limit,
            show: p.show,
            sell: p.sell.unwrap_or(true),
            renew: p.renew,
            reset_traffic_method: p.reset_traffic_method,
            sort: p.sort,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }

    pub fn get_period_key(period: &str) -> String {
        match period {
            "month_price" | "monthly" => "monthly".to_string(),
            "quarter_price" | "quarterly" => "quarterly".to_string(),
            "half_year_price" | "half_yearly" => "half_yearly".to_string(),
            "year_price" | "yearly" => "yearly".to_string(),
            "two_year_price" | "two_yearly" => "two_yearly".to_string(),
            "three_year_price" | "three_yearly" => "three_yearly".to_string(),
            "onetime_price" | "onetime" => "onetime".to_string(),
            "reset_price" | "reset_traffic" => "reset_traffic".to_string(),
            other => other.to_string(),
        }
    }

    pub fn get_legacy_period(period: &str) -> String {
        match period {
            "monthly" | "month_price" => "month_price".to_string(),
            "quarterly" | "quarter_price" => "quarter_price".to_string(),
            "half_yearly" | "half_year_price" => "half_year_price".to_string(),
            "yearly" | "year_price" => "year_price".to_string(),
            "two_yearly" | "two_year_price" => "two_year_price".to_string(),
            "three_yearly" | "three_year_price" => "three_year_price".to_string(),
            "onetime" | "onetime_price" => "onetime_price".to_string(),
            "reset_traffic" | "reset_price" => "reset_price".to_string(),
            other => other.to_string(),
        }
    }

    pub fn get_period_months(period: &str) -> Option<i64> {
        let key = Self::get_period_key(period);
        match key.as_str() {
            "monthly" => Some(1),
            "quarterly" => Some(3),
            "half_yearly" => Some(6),
            "yearly" => Some(12),
            "two_yearly" => Some(24),
            "three_yearly" => Some(36),
            _ => None,
        }
    }

    pub async fn has_capacity(&self, plan: &plan::Model) -> Result<bool, AppError> {
        if let Some(limit) = plan.capacity_limit {
            let now = Utc::now().timestamp();
            let active_count = User::find()
                .filter(user::Column::PlanId.eq(plan.id))
                .filter(
                    user::Column::ExpiredAt
                        .is_null()
                        .or(user::Column::ExpiredAt.gte(now)),
                )
                .all(&self.db)
                .await?
                .len() as i32;
            Ok(limit - active_count > 0)
        } else {
            Ok(true)
        }
    }

    pub fn validate_plan_availability(
        &self,
        user: &user::Model,
        plan: &plan::Model,
    ) -> Result<(), AppError> {
        let now = Utc::now().timestamp();
        let is_avail =
            !user.banned && (user.expired_at.is_none() || user.expired_at.unwrap() >= now);

        if (!plan.show && !plan.renew) || (!plan.show && user.plan_id != Some(plan.id)) {
            return Err(AppError::Custom(
                400,
                "该订阅已售罄，请选择其他订阅".to_string(),
            ));
        }

        if !plan.renew && user.plan_id == Some(plan.id) {
            return Err(AppError::Custom(
                400,
                "该订阅无法续费，请更换其他订阅".to_string(),
            ));
        }

        if !plan.show && plan.renew && !is_avail {
            return Err(AppError::Custom(
                400,
                "该订阅已过期，请更换其他订阅".to_string(),
            ));
        }

        Ok(())
    }

    pub async fn validate_purchase(
        &self,
        user: &user::Model,
        plan: &plan::Model,
        period: &str,
    ) -> Result<(), AppError> {
        let period_key = Self::get_period_key(period);
        let price = plan.get_price(&period_key);
        if price.is_none() {
            return Err(AppError::Custom(
                400,
                "该付款周期无法购买，请选择其他周期".to_string(),
            ));
        }

        if period_key == "reset_traffic" {
            let now = Utc::now().timestamp();
            let is_avail =
                !user.banned && (user.expired_at.is_none() || user.expired_at.unwrap() >= now);
            if !is_avail || user.plan_id != Some(plan.id) {
                return Err(AppError::Custom(
                    400,
                    "订阅已过期或无有效订阅，无法购买流量重置包".to_string(),
                ));
            }
            return Ok(());
        }

        if user.plan_id != Some(plan.id) && !self.has_capacity(plan).await? {
            return Err(AppError::Custom(400, "当前商品已售罄".to_string()));
        }

        self.validate_plan_availability(user, plan)?;

        Ok(())
    }
}
