use chrono::{Datelike, TimeZone, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};

use crate::{
    common::AppError,
    entities::{
        order, personal_access_token, plan, stat::user as stat_user, ticket, user, Order,
        PersonalAccessToken, StatUser, Ticket, User,
    },
    utils::{hash_password, md5_hex, random_char, verify_password},
};

#[derive(Clone, Default)]
pub struct UserService;

impl UserService {
    pub fn new() -> Self {
        Self
    }

    /// Checks whether a user account is active and eligible for subscriptions.
    pub fn is_available(user: &user::Model) -> bool {
        if user.banned {
            return false;
        }
        if user.transfer_enable <= 0 {
            return false;
        }
        let now = Utc::now().timestamp();
        if let Some(expired_at) = user.expired_at {
            if expired_at <= now {
                return false;
            }
        }
        true
    }

    /// Returns the Gravatar / avatar URL for a user email.
    pub fn get_avatar_url(email: &str) -> String {
        let hash = md5_hex(email.trim().to_lowercase().as_bytes());
        format!("https://cdn.v2ex.com/gravatar/{}?s=64&d=identicon", hash)
    }

    /// Changes user password, verifies old password, and clears other active tokens.
    pub async fn change_password(
        &self,
        db: &DatabaseConnection,
        user: &user::Model,
        old_password: &str,
        new_password: &str,
        current_token: Option<&str>,
    ) -> Result<bool, AppError> {
        if !verify_password(old_password, &user.password) {
            return Err(AppError::BadRequest("The old password is wrong".into()));
        }

        let hashed = hash_password(new_password)?;
        let mut active: user::ActiveModel = user.clone().into();
        active.password = Set(hashed);
        active.password_algo = Set(None);
        active.password_salt = Set(None);
        active.updated_at = Set(Utc::now().timestamp());
        active.update(db).await?;

        // Invalidate tokens except current token
        if let Some(tok) = current_token {
            let clean = tok.strip_prefix("Bearer ").unwrap_or(tok).trim();
            let tok_hash = crate::utils::sha256_hex(clean.as_bytes());
            PersonalAccessToken::delete_many()
                .filter(personal_access_token::Column::TokenableId.eq(user.id))
                .filter(personal_access_token::Column::Token.ne(tok_hash))
                .exec(db)
                .await?;
        } else {
            PersonalAccessToken::delete_many()
                .filter(personal_access_token::Column::TokenableId.eq(user.id))
                .exec(db)
                .await?;
        }

        Ok(true)
    }

    /// Resets security tokens (uuid and subscribe token).
    pub async fn reset_security(
        &self,
        db: &DatabaseConnection,
        user: &user::Model,
    ) -> Result<(String, String), AppError> {
        let new_uuid = uuid::Uuid::new_v4().to_string();
        let new_token = random_char(32, false);

        let mut active: user::ActiveModel = user.clone().into();
        active.uuid = Set(new_uuid.clone());
        active.token = Set(new_token.clone());
        active.updated_at = Set(Utc::now().timestamp());
        active.update(db).await?;

        Ok((new_uuid, new_token))
    }

    /// Updates user notification reminder preferences.
    pub async fn update_remind_settings(
        &self,
        db: &DatabaseConnection,
        user: &user::Model,
        remind_expire: Option<bool>,
        remind_traffic: Option<bool>,
    ) -> Result<bool, AppError> {
        let mut active: user::ActiveModel = user.clone().into();
        if let Some(re) = remind_expire {
            active.remind_expire = Set(Some(re));
        }
        if let Some(rt) = remind_traffic {
            active.remind_traffic = Set(Some(rt));
        }
        active.updated_at = Set(Utc::now().timestamp());
        active.update(db).await?;
        Ok(true)
    }

    /// Transfers commission balance into account balance in an atomic transaction.
    pub async fn transfer_commission(
        &self,
        db: &DatabaseConnection,
        user_id: i32,
        amount: i32,
    ) -> Result<bool, AppError> {
        if amount <= 0 {
            return Err(AppError::BadRequest("Invalid transfer amount".into()));
        }

        let txn = db.begin().await?;

        let u = User::find_by_id(user_id)
            .one(&txn)
            .await?
            .ok_or_else(|| AppError::BadRequest("The user does not exist".into()))?;

        if amount > u.commission_balance {
            return Err(AppError::BadRequest(
                "Insufficient commission balance".into(),
            ));
        }

        let mut active: user::ActiveModel = u.clone().into();
        active.commission_balance = Set(u.commission_balance - amount);
        active.balance = Set(u.balance + amount);
        active.updated_at = Set(Utc::now().timestamp());
        active.update(&txn).await?;

        txn.commit().await?;
        Ok(true)
    }

    /// Fetches user stats: [unpaid_orders_count, open_tickets_count, invited_users_count].
    pub async fn get_user_stat(
        &self,
        db: &DatabaseConnection,
        user_id: i32,
    ) -> Result<[i64; 3], AppError> {
        let unpaid_orders = Order::find()
            .filter(order::Column::UserId.eq(user_id))
            .filter(order::Column::Status.eq(0))
            .count(db)
            .await? as i64;

        let open_tickets = Ticket::find()
            .filter(ticket::Column::UserId.eq(user_id))
            .filter(ticket::Column::Status.eq(0))
            .count(db)
            .await? as i64;

        let invited_users = User::find()
            .filter(user::Column::InviteUserId.eq(user_id))
            .count(db)
            .await? as i64;

        Ok([unpaid_orders, open_tickets, invited_users])
    }

    /// Checks if a plan is available for a user (considering renewal vs new purchase).
    pub fn is_plan_available_for_user(
        &self,
        target_plan: &plan::Model,
        user: &user::Model,
    ) -> bool {
        if let Some(user_plan_id) = user.plan_id {
            if user_plan_id == target_plan.id {
                return target_plan.renew;
            }
        }
        target_plan.show && target_plan.sell.unwrap_or(true)
    }

    /// Calculates remaining days until next traffic reset.
    pub fn get_reset_day(&self, user: &user::Model, plan: Option<&plan::Model>) -> Option<i32> {
        let now = Utc::now().timestamp();
        if let Some(next_reset) = user.next_reset_at {
            if next_reset > now {
                let days = ((next_reset - now + 86399) / 86400) as i32;
                return Some(days);
            }
            return Some(0);
        }

        let plan = plan?;
        let method = plan.reset_traffic_method.unwrap_or(1);
        if method == 2 {
            // NEVER
            return None;
        }

        let now_dt = Utc::now();
        match method {
            0 => {
                // First day of next month
                let next_month_year = if now_dt.month() == 12 {
                    now_dt.year() + 1
                } else {
                    now_dt.year()
                };
                let next_month = if now_dt.month() == 12 {
                    1
                } else {
                    now_dt.month() + 1
                };
                let target = Utc
                    .with_ymd_and_hms(next_month_year, next_month, 1, 0, 0, 0)
                    .single()?;
                let diff = (target.timestamp() - now + 86399) / 86400;
                Some(diff as i32)
            }
            _ => {
                // Monthly based on expired_at or day 1
                if let Some(exp) = user.expired_at {
                    if let Some(exp_dt) = Utc.timestamp_opt(exp, 0).single() {
                        let day = exp_dt.day();
                        let target = Utc
                            .with_ymd_and_hms(now_dt.year(), now_dt.month(), day.min(28), 0, 0, 0)
                            .single()?;
                        if target.timestamp() > now {
                            let diff = (target.timestamp() - now + 86399) / 86400;
                            return Some(diff as i32);
                        }
                    }
                }
                None
            }
        }
    }

    /// Fetches user traffic consumption logs for the current month.
    pub async fn get_monthly_traffic_logs(
        &self,
        db: &DatabaseConnection,
        user_id: i32,
    ) -> Result<Vec<stat_user::Model>, AppError> {
        let now_dt = Utc::now();
        let start_of_month = Utc
            .with_ymd_and_hms(now_dt.year(), now_dt.month(), 1, 0, 0, 0)
            .single()
            .map(|dt| dt.timestamp())
            .unwrap_or(0);

        let logs = StatUser::find()
            .filter(stat_user::Column::UserId.eq(user_id))
            .filter(stat_user::Column::RecordAt.gte(start_of_month))
            .order_by_desc(stat_user::Column::RecordAt)
            .all(db)
            .await?;

        Ok(logs)
    }

    /// Creates a new admin account or resets password for an existing account and grants admin privileges.
    pub async fn create_or_reset_admin(
        &self,
        db: &DatabaseConnection,
        email: &str,
        password: &str,
    ) -> Result<user::Model, AppError> {
        let hashed = hash_password(password)?;
        let email_clean = email.trim().to_lowercase();
        let now = Utc::now().timestamp();

        let existing = User::find()
            .filter(user::Column::Email.eq(&email_clean))
            .one(db)
            .await?;

        if let Some(user_model) = existing {
            let mut active: user::ActiveModel = user_model.into();
            active.password = Set(hashed);
            active.is_admin = Set(true);
            active.is_staff = Set(true);
            active.banned = Set(false);
            active.updated_at = Set(now);
            let updated = active.update(db).await?;
            Ok(updated)
        } else {
            let new_admin = user::ActiveModel {
                email: Set(email_clean),
                password: Set(hashed),
                is_admin: Set(true),
                is_staff: Set(true),
                banned: Set(false),
                balance: Set(0),
                commission_balance: Set(0),
                commission_type: Set(0),
                transfer_enable: Set(1000 * 1073741824), // 1000 GB
                u: Set(0),
                d: Set(0),
                t: Set(0),
                uuid: Set(crate::utils::generate_uuid()),
                token: Set(crate::utils::generate_uuid()),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            };
            let created = new_admin.insert(db).await?;
            Ok(created)
        }
    }

    /// Resets an existing user's password without modifying any administrative or staff roles.
    /// Also clears all active access tokens for this user, forcing re-authentication.
    pub async fn reset_user_password(
        &self,
        db: &DatabaseConnection,
        email: &str,
        new_password: &str,
    ) -> Result<user::Model, AppError> {
        let email_clean = email.trim().to_lowercase();
        let user_model = User::find()
            .filter(user::Column::Email.eq(&email_clean))
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("User '{}' does not exist", email_clean)))?;

        let hashed = hash_password(new_password)?;
        let now = Utc::now().timestamp();

        let mut active: user::ActiveModel = user_model.into();
        active.password = Set(hashed);
        active.password_algo = Set(None);
        active.password_salt = Set(None);
        active.updated_at = Set(now);
        let updated = active.update(db).await?;

        // Invalidate all tokens for this user
        PersonalAccessToken::delete_many()
            .filter(personal_access_token::Column::TokenableId.eq(updated.id))
            .exec(db)
            .await?;

        Ok(updated)
    }
}
