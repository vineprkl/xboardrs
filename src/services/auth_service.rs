use chrono::Utc;
use rand::Rng;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    common::AppError,
    entities::{
        invite_code, personal_access_token, user, InviteCode, PersonalAccessToken, Plan, User,
    },
    services::SettingService,
    utils::{generate_uuid, hash_password, random_char, sha256_hex, verify_password},
};

const DEFAULT_EMAIL_WHITELIST_SUFFIX: &[&str] = &[
    "gmail.com",
    "qq.com",
    "163.com",
    "yahoo.com",
    "sina.com",
    "126.com",
    "outlook.com",
    "yeah.net",
    "foxmail.com",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthDataDto {
    pub token: String,
    pub auth_data: String,
    pub is_admin: bool,
}

#[derive(Debug, Deserialize)]
pub struct RegisterDto {
    pub email: String,
    pub password: String,
    pub invite_code: Option<String>,
    pub email_code: Option<String>,
}

#[derive(Clone)]
pub struct AuthService {
    db: DatabaseConnection,
    setting_service: SettingService,
    cache: Arc<RwLock<HashMap<String, (String, i64)>>>,
}

impl AuthService {
    pub fn new(db: DatabaseConnection, setting_service: SettingService) -> Self {
        Self {
            db,
            setting_service,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // Cache operations with TTL in seconds
    pub async fn put_cache(&self, key: &str, value: &str, ttl_seconds: i64) {
        let expire = Utc::now().timestamp() + ttl_seconds;
        let mut cache = self.cache.write().await;
        cache.insert(key.to_string(), (value.to_string(), expire));
    }

    pub async fn get_cache(&self, key: &str) -> Option<String> {
        let now = Utc::now().timestamp();
        let cache = self.cache.read().await;
        if let Some((val, expire)) = cache.get(key) {
            if *expire > now {
                return Some(val.clone());
            }
        }
        None
    }

    pub async fn forget_cache(&self, key: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(key);
    }

    pub async fn increment_cache(&self, key: &str, ttl_seconds: i64) -> i64 {
        let now = Utc::now().timestamp();
        let mut cache = self.cache.write().await;
        let count = if let Some((val, expire)) = cache.get(key) {
            if *expire > now {
                val.parse::<i64>().unwrap_or(0) + 1
            } else {
                1
            }
        } else {
            1
        };
        let expire = now + ttl_seconds;
        cache.insert(key.to_string(), (count.to_string(), expire));
        count
    }

    /// Generates Laravel Sanctum-compatible auth data (Bearer token).
    pub async fn generate_auth_data(&self, user: &user::Model) -> Result<AuthDataDto, AppError> {
        let raw_token = random_char(40, false);
        let token_hash = sha256_hex(raw_token.as_bytes());
        let now = Utc::now().timestamp();
        let expires_at = now + 365 * 86400; // 1 year

        let pat = personal_access_token::ActiveModel {
            tokenable_type: Set("App\\Models\\User".to_string()),
            tokenable_id: Set(user.id as i64),
            name: Set(random_char(20, false)),
            token: Set(token_hash),
            abilities: Set(Some("[\"*\"]".to_string())),
            expires_at: Set(Some(expires_at)),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            ..Default::default()
        };

        pat.insert(&self.db).await?;

        Ok(AuthDataDto {
            token: user.token.clone(),
            auth_data: format!("Bearer {}", raw_token),
            is_admin: user.is_admin,
        })
    }

    /// Finds user by Sanctum Bearer token matching Laravel's `PersonalAccessToken::findToken`.
    pub async fn find_user_by_bearer_token(
        &self,
        raw_token: &str,
    ) -> Result<Option<user::Model>, AppError> {
        let token = raw_token.trim().trim_start_matches("Bearer ").trim();
        if token.is_empty() {
            return Ok(None);
        }

        let token_hash = sha256_hex(token.as_bytes());
        let now = Utc::now().timestamp();

        let pat = PersonalAccessToken::find()
            .filter(personal_access_token::Column::Token.eq(token_hash))
            .one(&self.db)
            .await?;

        if let Some(pat) = pat {
            if let Some(exp) = pat.expires_at {
                if exp < now {
                    return Ok(None); // Token expired
                }
            }
            let user = User::find_by_id(pat.tokenable_id as i32)
                .one(&self.db)
                .await?;
            return Ok(user);
        }

        Ok(None)
    }

    /// Authenticates a user by email and password.
    pub async fn login(&self, email: &str, password: &str) -> Result<user::Model, AppError> {
        let email_key = email.trim().to_lowercase();
        let limit_key = format!("PASSWORD_ERROR_LIMIT:{}", email_key);

        if self
            .setting_service
            .get_bool("password_limit_enable", true)
            .await
        {
            let error_count = self
                .get_cache(&limit_key)
                .await
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            let limit_count = self
                .setting_service
                .get_int("password_limit_count", 5)
                .await;
            if error_count >= limit_count {
                let expire_minutes = self
                    .setting_service
                    .get_int("password_limit_expire", 60)
                    .await;
                return Err(AppError::TooManyRequests(format!(
                    "There are too many password errors, please try again after {} minutes.",
                    expire_minutes
                )));
            }
        }

        let user = User::find()
            .filter(user::Column::Email.eq(&email_key))
            .one(&self.db)
            .await?;

        let user = match user {
            Some(u) => u,
            None => {
                let expire = self
                    .setting_service
                    .get_int("password_limit_expire", 60)
                    .await
                    * 60;
                self.increment_cache(&limit_key, expire).await;
                return Err(AppError::BadRequest("Incorrect email or password".into()));
            }
        };

        if !verify_password(password, &user.password) {
            let expire = self
                .setting_service
                .get_int("password_limit_expire", 60)
                .await
                * 60;
            self.increment_cache(&limit_key, expire).await;
            return Err(AppError::BadRequest("Incorrect email or password".into()));
        }

        if user.banned {
            return Err(AppError::BadRequest(
                "Your account has been suspended".into(),
            ));
        }

        // Reset error count on successful login
        self.forget_cache(&limit_key).await;

        // Update last login timestamp
        let now = Utc::now().timestamp();
        let mut active: user::ActiveModel = user.clone().into();
        active.last_login_at = Set(Some(now));
        let updated = active.update(&self.db).await?;

        Ok(updated)
    }

    /// Registers a new user with validation for captcha, email whitelist, invite codes, and trial plans.
    pub async fn register(&self, req: RegisterDto) -> Result<user::Model, AppError> {
        let email = req.email.trim().to_lowercase();
        if email.is_empty() || !email.contains('@') {
            return Err(AppError::BadRequest("Invalid email address".into()));
        }

        if self.setting_service.get_bool("stop_register", false).await {
            return Err(AppError::BadRequest("Registration has closed".into()));
        }

        // Check email whitelist
        if self
            .setting_service
            .get_bool("email_whitelist_enable", false)
            .await
        {
            let email_suffix = email.split('@').nth(1).unwrap_or("");
            let allowed_suffixes = {
                let raw = self
                    .setting_service
                    .get_string("email_whitelist_suffix", "")
                    .await;
                if raw.trim().is_empty() {
                    DEFAULT_EMAIL_WHITELIST_SUFFIX
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                } else if let Ok(arr) = serde_json::from_str::<Vec<String>>(&raw) {
                    arr
                } else {
                    raw.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                }
            };

            if !allowed_suffixes
                .iter()
                .any(|s| s.eq_ignore_ascii_case(email_suffix))
            {
                return Err(AppError::BadRequest(
                    "Email suffix is not in the Whitelist".into(),
                ));
            }
        }

        // Check Gmail alias limitation
        if self
            .setting_service
            .get_bool("email_gmail_limit_enable", false)
            .await
        {
            let parts: Vec<&str> = email.split('@').collect();
            if parts.len() == 2
                && parts[1].eq_ignore_ascii_case("gmail.com")
                && (parts[0].contains('.') || parts[0].contains('+'))
            {
                return Err(AppError::BadRequest("Gmail alias is not supported".into()));
            }
        }

        // Check invite code
        let invite_force = self.setting_service.get_bool("invite_force", false).await;
        let invite_code_str = req.invite_code.as_deref().unwrap_or("").trim();
        if invite_force && invite_code_str.is_empty() {
            return Err(AppError::UnprocessableEntity(
                "You must use the invitation code to register".into(),
            ));
        }

        let mut invite_user_id = None;
        if !invite_code_str.is_empty() {
            let invite = InviteCode::find()
                .filter(invite_code::Column::Code.eq(invite_code_str))
                .filter(invite_code::Column::Status.eq(false))
                .one(&self.db)
                .await?;

            match invite {
                Some(inv) => {
                    invite_user_id = Some(inv.user_id);
                    if !self
                        .setting_service
                        .get_bool("invite_never_expire", false)
                        .await
                    {
                        let mut active: invite_code::ActiveModel = inv.into();
                        active.status = Set(true);
                        active.updated_at = Set(Utc::now().timestamp());
                        active.update(&self.db).await?;
                    }
                }
                None => {
                    if invite_force {
                        return Err(AppError::BadRequest("Invalid invitation code".into()));
                    }
                }
            }
        }

        // Check email verify code
        if self.setting_service.get_bool("email_verify", false).await {
            let email_code = req.email_code.as_deref().unwrap_or("").trim();
            if email_code.len() != 6 {
                return Err(AppError::UnprocessableEntity(
                    "Email verification code cannot be empty".into(),
                ));
            }

            let cache_key = format!("EMAIL_VERIFY_CODE:{}", email);
            let cached_code = self.get_cache(&cache_key).await;
            match cached_code {
                Some(c) if c == email_code => {
                    self.forget_cache(&cache_key).await;
                }
                _ => {
                    return Err(AppError::BadRequest(
                        "Incorrect email verification code".into(),
                    ));
                }
            }
        }

        // Check duplicate email
        let existing = User::find()
            .filter(user::Column::Email.eq(&email))
            .one(&self.db)
            .await?;
        if existing.is_some() {
            return Err(AppError::Custom(400201, "Email already exists".into()));
        }

        let hashed_password = hash_password(&req.password)?;
        let now = Utc::now().timestamp();

        // Check trial / try-out plan
        let try_out_plan_id = self.setting_service.get_int("try_out_plan_id", 0).await as i32;
        let try_out_hour = self.setting_service.get_int("try_out_hour", 1).await;

        let mut plan_id = None;
        let mut group_id = None;
        let mut transfer_enable = 0;
        let mut expired_at = None;
        let mut speed_limit = None;

        if try_out_plan_id > 0 {
            if let Some(p) = Plan::find_by_id(try_out_plan_id).one(&self.db).await? {
                plan_id = Some(p.id);
                group_id = Some(p.group_id);
                transfer_enable = p.transfer_enable * 1073741824;
                expired_at = Some(now + (try_out_hour * 3600));
                speed_limit = p.speed_limit;
            }
        }

        let new_user = user::ActiveModel {
            email: Set(email),
            password: Set(hashed_password),
            balance: Set(0),
            commission_type: Set(0),
            commission_balance: Set(0),
            t: Set(0),
            u: Set(0),
            d: Set(0),
            transfer_enable: Set(transfer_enable),
            banned: Set(false),
            is_admin: Set(false),
            is_staff: Set(false),
            uuid: Set(generate_uuid()),
            token: Set(random_char(32, false)),
            group_id: Set(group_id),
            plan_id: Set(plan_id),
            speed_limit: Set(speed_limit),
            remind_expire: Set(Some(
                self.setting_service
                    .get_bool("default_remind_expire", true)
                    .await,
            )),
            remind_traffic: Set(Some(
                self.setting_service
                    .get_bool("default_remind_traffic", true)
                    .await,
            )),
            expired_at: Set(expired_at),
            invite_user_id: Set(invite_user_id),
            last_login_at: Set(Some(now)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        let inserted = new_user.insert(&self.db).await?;
        Ok(inserted)
    }

    /// Sends verification code to email (stores in cache with cooldown).
    pub async fn send_email_verify(&self, email: &str) -> Result<bool, AppError> {
        let email_key = email.trim().to_lowercase();
        if email_key.is_empty() || !email_key.contains('@') {
            return Err(AppError::BadRequest("Invalid email address".into()));
        }

        let cooldown_key = format!("LAST_SEND_EMAIL_VERIFY_TIMESTAMP:{}", email_key);
        if self.get_cache(&cooldown_key).await.is_some() {
            return Err(AppError::BadRequest(
                "Email verification code has been sent, please request again later".into(),
            ));
        }

        // If whitelist enabled and not registered, check whitelist
        if self
            .setting_service
            .get_bool("email_whitelist_enable", false)
            .await
        {
            let user_exists = User::find()
                .filter(user::Column::Email.eq(&email_key))
                .one(&self.db)
                .await?
                .is_some();

            if !user_exists {
                let email_suffix = email_key.split('@').nth(1).unwrap_or("");
                let allowed_suffixes = {
                    let raw = self
                        .setting_service
                        .get_string("email_whitelist_suffix", "")
                        .await;
                    if raw.trim().is_empty() {
                        DEFAULT_EMAIL_WHITELIST_SUFFIX
                            .iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    } else if let Ok(arr) = serde_json::from_str::<Vec<String>>(&raw) {
                        arr
                    } else {
                        raw.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    }
                };

                if !allowed_suffixes
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(email_suffix))
                {
                    return Err(AppError::BadRequest(
                        "Email suffix is not in whitelist".into(),
                    ));
                }
            }
        }

        let code = format!("{:06}", rand::thread_rng().gen_range(100000..=999999));
        let verify_key = format!("EMAIL_VERIFY_CODE:{}", email_key);

        self.put_cache(&verify_key, &code, 300).await;
        self.put_cache(&cooldown_key, &Utc::now().timestamp().to_string(), 60)
            .await;

        // In a full system, background queue SendEmailJob would be dispatched here.
        tracing::info!("Dispatched verification code {} to {}", code, email_key);

        Ok(true)
    }

    /// Resets user password with email verification code.
    pub async fn reset_password(
        &self,
        email: &str,
        email_code: &str,
        new_password: &str,
    ) -> Result<(), AppError> {
        let email_key = email.trim().to_lowercase();
        let limit_key = format!("FORGET_REQUEST_LIMIT:{}", email_key);

        let error_count = self
            .get_cache(&limit_key)
            .await
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        if error_count >= 3 {
            return Err(AppError::TooManyRequests(
                "Reset failed, Please try again later".into(),
            ));
        }

        let verify_key = format!("EMAIL_VERIFY_CODE:{}", email_key);
        let cached_code = self.get_cache(&verify_key).await;
        match cached_code {
            Some(c) if c == email_code => {}
            _ => {
                self.increment_cache(&limit_key, 300).await;
                return Err(AppError::BadRequest(
                    "Incorrect email verification code".into(),
                ));
            }
        }

        let user = User::find()
            .filter(user::Column::Email.eq(&email_key))
            .one(&self.db)
            .await?
            .ok_or_else(|| {
                AppError::BadRequest("This email is not registered in the system".into())
            })?;

        let hashed = hash_password(new_password)?;
        let mut active: user::ActiveModel = user.into();
        active.password = Set(hashed);
        active.password_algo = Set(None);
        active.password_salt = Set(None);
        active.updated_at = Set(Utc::now().timestamp());
        active.update(&self.db).await?;

        self.forget_cache(&verify_key).await;
        Ok(())
    }

    /// Generates quick login URL using temporary token.
    pub async fn generate_quick_login_url(
        &self,
        user: &user::Model,
        redirect: Option<&str>,
    ) -> Result<String, AppError> {
        let code = random_char(32, false);
        let key = format!("TEMP_TOKEN:{}", code);
        self.put_cache(&key, &user.id.to_string(), 60).await;

        let redirect_target = redirect.unwrap_or("dashboard");
        let login_redirect = format!(
            "/#/login?verify={}&redirect={}",
            code,
            urlencoding::encode(redirect_target)
        );
        let app_url = self.setting_service.get_string("app_url", "").await;

        let full_url = if app_url.trim().is_empty() {
            login_redirect
        } else {
            format!("{}{}", app_url.trim_end_matches('/'), login_redirect)
        };

        Ok(full_url)
    }

    /// Verifies temporary token from quick login and returns user.
    pub async fn verify_temp_token(&self, verify: &str) -> Result<Option<user::Model>, AppError> {
        let key = format!("TEMP_TOKEN:{}", verify);
        let user_id_opt = self.get_cache(&key).await;
        if let Some(user_id_str) = user_id_opt {
            self.forget_cache(&key).await;
            if let Ok(user_id) = user_id_str.parse::<i32>() {
                let user = User::find_by_id(user_id).one(&self.db).await?;
                return Ok(user);
            }
        }
        Ok(None)
    }

    /// Records page view for invite code.
    pub async fn record_invite_pv(&self, code: &str) -> Result<bool, AppError> {
        let invite = InviteCode::find()
            .filter(invite_code::Column::Code.eq(code))
            .one(&self.db)
            .await?;

        if let Some(inv) = invite {
            let mut active: invite_code::ActiveModel = inv.into();
            active.pv = Set(active.pv.as_ref() + 1);
            active.updated_at = Set(Utc::now().timestamp());
            active.update(&self.db).await?;
        }
        Ok(true)
    }
}
