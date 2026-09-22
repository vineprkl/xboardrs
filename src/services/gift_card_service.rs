use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    common::AppError,
    entities::{
        gift_card::{code, usage},
        user, GiftCardCode, GiftCardTemplate, GiftCardUsage, Plan, User,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftCardRewards {
    pub balance: Option<i32>,
    pub transfer_enable: Option<i64>,
    pub device_limit: Option<i32>,
    pub plan_id: Option<i32>,
    pub plan_validity_days: Option<i64>,
    pub expire_days: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftCardCheckResult {
    pub code_info: serde_json::Value,
    pub reward_preview: serde_json::Value,
    pub can_redeem: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftCardRedeemResult {
    pub message: String,
    pub rewards: serde_json::Value,
    pub invite_rewards: Option<serde_json::Value>,
    pub template_name: String,
}

#[derive(Clone)]
pub struct GiftCardService {
    db: DatabaseConnection,
}

impl GiftCardService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Returns mapped gift card template types.
    pub fn get_type_map() -> serde_json::Value {
        json!({
            "1": "通用礼品卡",
            "2": "套餐礼品卡",
            "3": "盲盒礼品卡"
        })
    }

    /// Checks whether a gift card code is valid and available for the current user.
    pub async fn check_code(
        &self,
        code_str: &str,
        _user: &user::Model,
    ) -> Result<GiftCardCheckResult, AppError> {
        let code_str = code_str.trim();
        if code_str.is_empty() {
            return Err(AppError::UnprocessableEntity("兑换码不能为空".into()));
        }

        let code_model = GiftCardCode::find()
            .filter(code::Column::Code.eq(code_str))
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::BadRequest("兑换码不存在".into()))?;

        let template_model = GiftCardTemplate::find_by_id(code_model.template_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::BadRequest("该礼品卡类型已停用".into()))?;

        if template_model.status != 1 {
            return Err(AppError::BadRequest("该礼品卡类型已停用".into()));
        }

        if code_model.status != 0 {
            let status_name = match code_model.status {
                1 => "已使用",
                2 => "已过期",
                3 => "已禁用",
                _ => "不可用",
            };
            return Err(AppError::BadRequest(format!(
                "兑换码不可用：{}",
                status_name
            )));
        }

        let now = Utc::now().timestamp();
        if let Some(exp) = code_model.expires_at {
            if exp < now {
                return Err(AppError::BadRequest("兑换码不可用：已过期".into()));
            }
        }

        let rewards_val: serde_json::Value =
            serde_json::from_str(&template_model.rewards).unwrap_or(json!({}));

        let code_info = json!({
            "code": code_model.code,
            "template_name": template_model.name,
            "template_type": template_model.r#type,
            "theme_color": template_model.theme_color,
            "icon": template_model.icon,
        });

        // Check user conditions if defined
        let can_redeem = true;
        let reason = None;

        Ok(GiftCardCheckResult {
            code_info,
            reward_preview: rewards_val,
            can_redeem,
            reason,
        })
    }

    /// Atomically redeems a gift card code for a user and applies all rewards.
    pub async fn redeem_code(
        &self,
        code_str: &str,
        user: &user::Model,
        user_agent: Option<&str>,
    ) -> Result<GiftCardRedeemResult, AppError> {
        let code_str = code_str.trim();
        if code_str.is_empty() {
            return Err(AppError::UnprocessableEntity("兑换码不能为空".into()));
        }

        let txn = self.db.begin().await?;

        let code_model = GiftCardCode::find()
            .filter(code::Column::Code.eq(code_str))
            .one(&txn)
            .await?
            .ok_or_else(|| AppError::BadRequest("兑换码不存在".into()))?;

        let template_model = GiftCardTemplate::find_by_id(code_model.template_id)
            .one(&txn)
            .await?
            .ok_or_else(|| AppError::BadRequest("该礼品卡类型不存在".into()))?;

        if template_model.status != 1 {
            return Err(AppError::BadRequest("该礼品卡类型已停用".into()));
        }

        if code_model.status != 0 {
            let status_name = match code_model.status {
                1 => "已使用",
                2 => "已过期",
                3 => "已禁用",
                _ => "不可用",
            };
            return Err(AppError::BadRequest(format!(
                "兑换码不可用：{}",
                status_name
            )));
        }

        let now = Utc::now().timestamp();
        if let Some(exp) = code_model.expires_at {
            if exp < now {
                return Err(AppError::BadRequest("兑换码不可用：已过期".into()));
            }
        }

        let rewards_val: serde_json::Value =
            serde_json::from_str(&template_model.rewards).unwrap_or(json!({}));
        let rewards: GiftCardRewards =
            serde_json::from_value(rewards_val.clone()).unwrap_or(GiftCardRewards {
                balance: None,
                transfer_enable: None,
                device_limit: None,
                plan_id: None,
                plan_validity_days: None,
                expire_days: None,
            });

        // Re-query user inside transaction
        let current_user = User::find_by_id(user.id)
            .one(&txn)
            .await?
            .ok_or_else(|| AppError::BadRequest("User not found".into()))?;

        let mut user_active: user::ActiveModel = current_user.clone().into();

        // 1. Balance reward
        if let Some(bal) = rewards.balance {
            if bal > 0 {
                let current_bal = current_user.balance;
                user_active.balance = Set(current_bal + bal);
            }
        }

        // 2. Transfer enable reward
        if let Some(traffic) = rewards.transfer_enable {
            if traffic > 0 {
                let current_traffic = current_user.transfer_enable;
                user_active.transfer_enable = Set(current_traffic + traffic);
            }
        }

        // 3. Device limit reward
        if let Some(dl) = rewards.device_limit {
            if dl > 0 {
                let current_dl = current_user.device_limit.unwrap_or(0);
                user_active.device_limit = Set(Some(current_dl + dl));
            }
        }

        // 4. Plan assignment
        if let Some(pid) = rewards.plan_id {
            if let Some(target_plan) = Plan::find_by_id(pid).one(&txn).await? {
                user_active.plan_id = Set(Some(target_plan.id));
                user_active.group_id = Set(Some(target_plan.group_id));
                user_active.speed_limit = Set(target_plan.speed_limit);
                user_active.transfer_enable = Set(target_plan.transfer_enable * 1073741824);

                let validity_days = rewards.plan_validity_days.unwrap_or(30);
                let current_exp = current_user.expired_at.unwrap_or(0);
                let base_time = current_exp.max(now);
                user_active.expired_at = Set(Some(base_time + (validity_days * 86400)));
            }
        } else if let Some(exp_days) = rewards.expire_days {
            if exp_days > 0 {
                let current_exp = current_user.expired_at.unwrap_or(0);
                let base_time = current_exp.max(now);
                user_active.expired_at = Set(Some(base_time + (exp_days * 86400)));
            }
        }

        user_active.updated_at = Set(now);
        user_active.update(&txn).await?;

        // Mark code as used
        let mut code_active: code::ActiveModel = code_model.clone().into();
        code_active.status = Set(1); // Used
        code_active.user_id = Set(Some(user.id));
        code_active.used_at = Set(Some(now));
        code_active.usage_count = Set(code_model.usage_count + 1);
        code_active.updated_at = Set(now);
        code_active.update(&txn).await?;

        // Insert usage record
        let usage_record = usage::ActiveModel {
            code_id: Set(code_model.id),
            template_id: Set(template_model.id),
            user_id: Set(user.id),
            invite_user_id: Set(user.invite_user_id),
            rewards_given: Set(rewards_val.to_string()),
            invite_rewards: Set(None),
            user_level_at_use: Set(None),
            plan_id_at_use: Set(current_user.plan_id),
            multiplier_applied: Set(1.0f64),
            ip_address: Set(None),
            user_agent: Set(user_agent.map(|s| s.to_string())),
            notes: Set(None),
            created_at: Set(now),
            ..Default::default()
        };
        usage_record.insert(&txn).await?;

        txn.commit().await?;

        Ok(GiftCardRedeemResult {
            message: "兑换成功！".to_string(),
            rewards: rewards_val,
            invite_rewards: None,
            template_name: template_model.name,
        })
    }

    /// Fetches user redemption history paginated.
    pub async fn fetch_history(
        &self,
        user_id: i32,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<serde_json::Value>, u64, u64), AppError> {
        let paginator = GiftCardUsage::find()
            .filter(usage::Column::UserId.eq(user_id))
            .order_by_desc(usage::Column::CreatedAt)
            .paginate(&self.db, per_page);

        let total = paginator.num_items().await?;
        let last_page = paginator.num_pages().await?;
        let records = paginator.fetch_page(page.saturating_sub(1)).await?;

        let mut data = Vec::new();
        for r in records {
            let code_str =
                if let Ok(Some(c)) = GiftCardCode::find_by_id(r.code_id).one(&self.db).await {
                    if c.code.len() >= 8 {
                        format!("{}****", &c.code[..8])
                    } else {
                        format!("{}****", c.code)
                    }
                } else {
                    "".to_string()
                };

            let (tmpl_name, tmpl_type) = if let Ok(Some(t)) =
                GiftCardTemplate::find_by_id(r.template_id)
                    .one(&self.db)
                    .await
            {
                let t_name = match t.r#type {
                    1 => "通用礼品卡",
                    2 => "套餐礼品卡",
                    3 => "盲盒礼品卡",
                    _ => "",
                };
                (t.name, t_name.to_string())
            } else {
                ("".to_string(), "".to_string())
            };

            let rewards_json: serde_json::Value =
                serde_json::from_str(&r.rewards_given).unwrap_or(json!({}));

            data.push(json!({
                "id": r.id,
                "code": code_str,
                "template_name": tmpl_name,
                "template_type": r.template_id,
                "template_type_name": tmpl_type,
                "rewards_given": rewards_json,
                "invite_rewards": r.invite_rewards.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
                "multiplier_applied": r.multiplier_applied.to_string(),
                "created_at": r.created_at,
            }));
        }

        Ok((data, total, last_page))
    }

    /// Fetches detail of a single gift card redemption record.
    pub async fn fetch_detail(
        &self,
        user_id: i32,
        usage_id: i32,
    ) -> Result<serde_json::Value, AppError> {
        let usage_record = GiftCardUsage::find_by_id(usage_id)
            .filter(usage::Column::UserId.eq(user_id))
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound("记录不存在".into()))?;

        let code_obj = GiftCardCode::find_by_id(usage_record.code_id)
            .one(&self.db)
            .await?;
        let tmpl_obj = GiftCardTemplate::find_by_id(usage_record.template_id)
            .one(&self.db)
            .await?;

        let tmpl_name = tmpl_obj
            .as_ref()
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let tmpl_desc = tmpl_obj.as_ref().and_then(|t| t.description.clone());
        let tmpl_type = tmpl_obj.as_ref().map(|t| t.r#type).unwrap_or(1);
        let tmpl_type_name = match tmpl_type {
            1 => "通用礼品卡",
            2 => "套餐礼品卡",
            3 => "盲盒礼品卡",
            _ => "",
        };

        let rewards_json: serde_json::Value =
            serde_json::from_str(&usage_record.rewards_given).unwrap_or(json!({}));

        Ok(json!({
            "id": usage_record.id,
            "code": code_obj.map(|c| c.code).unwrap_or_default(),
            "template": {
                "name": tmpl_name,
                "description": tmpl_desc,
                "type": tmpl_type,
                "type_name": tmpl_type_name,
                "icon": tmpl_obj.as_ref().and_then(|t| t.icon.clone()),
                "theme_color": tmpl_obj.as_ref().map(|t| t.theme_color.clone()).unwrap_or_default(),
            },
            "rewards_given": rewards_json,
            "invite_rewards": usage_record.invite_rewards.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
            "invite_user": null,
            "user_level_at_use": usage_record.user_level_at_use,
            "plan_id_at_use": usage_record.plan_id_at_use,
            "multiplier_applied": usage_record.multiplier_applied.to_string(),
            "notes": usage_record.notes,
            "created_at": usage_record.created_at,
        }))
    }
}
