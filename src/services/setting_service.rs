use crate::{
    common::AppError,
    entities::{setting, Setting},
    utils::crc32b,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::de::DeserializeOwned;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

/// Thread-safe cached system configuration service matching Laravel's `admin_setting()`.
#[derive(Clone)]
pub struct SettingService {
    db: DatabaseConnection,
    cache: Arc<RwLock<HashMap<String, String>>>,
}

impl SettingService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Loads all settings from `v2_settings` into memory.
    pub async fn load_all(&self) -> Result<(), AppError> {
        let list = Setting::find().all(&self.db).await?;
        let mut map = HashMap::new();
        for item in list {
            if let Some(val) = item.value {
                map.insert(item.name, val);
            }
        }
        let mut cache = self.cache.write().await;
        *cache = map;
        Ok(())
    }

    /// Returns a string setting or the provided default.
    pub async fn get_string(&self, key: &str, default: &str) -> String {
        let cache = self.cache.read().await;
        cache
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    /// Returns an optional string setting (None if key missing or value is empty).
    pub async fn get_optional_string(&self, key: &str) -> Option<String> {
        let cache = self.cache.read().await;
        cache.get(key).cloned().filter(|s| !s.trim().is_empty())
    }

    /// Returns an integer setting or the provided default.
    pub async fn get_int(&self, key: &str, default: i64) -> i64 {
        let cache = self.cache.read().await;
        cache
            .get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(default)
    }

    /// Returns a float setting or the provided default.
    pub async fn get_float(&self, key: &str, default: f64) -> f64 {
        let cache = self.cache.read().await;
        cache
            .get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(default)
    }

    /// Returns a boolean setting or the provided default (supports "1", "true", "0", "false").
    pub async fn get_bool(&self, key: &str, default: bool) -> bool {
        let cache = self.cache.read().await;
        if let Some(val) = cache.get(key) {
            match val.trim().to_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => true,
                "0" | "false" | "no" | "off" => false,
                _ => default,
            }
        } else {
            default
        }
    }

    /// Returns a decoded JSON value or None.
    pub async fn get_json<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let cache = self.cache.read().await;
        let raw = cache.get(key)?;
        serde_json::from_str(raw).ok()
    }

    /// Returns a snapshot copy of all currently cached settings.
    pub async fn get_all(&self) -> HashMap<String, String> {
        self.cache.read().await.clone()
    }

    /// Sets/updates a setting in both the database and the cache.
    pub async fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        let now = chrono::Utc::now().timestamp();
        let existing = Setting::find()
            .filter(setting::Column::Name.eq(key))
            .one(&self.db)
            .await?;

        if let Some(record) = existing {
            let mut active: setting::ActiveModel = record.into();
            active.value = Set(Some(value.to_string()));
            active.updated_at = Set(Some(now));
            active.update(&self.db).await?;
        } else {
            let active = setting::ActiveModel {
                name: Set(key.to_string()),
                value: Set(Some(value.to_string())),
                created_at: Set(Some(now)),
                updated_at: Set(Some(now)),
                ..Default::default()
            };
            active.insert(&self.db).await?;
        }

        let mut cache = self.cache.write().await;
        cache.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Resolves the secure admin panel path matching:
    /// `admin_setting('secure_path', admin_setting('frontend_admin_path', hash('crc32b', config('app.key'))))`
    pub async fn get_secure_path(&self, app_key: &str) -> String {
        let cache = self.cache.read().await;
        if let Some(p) = cache.get("secure_path") {
            if !p.trim().is_empty() {
                return p.clone();
            }
        }
        if let Some(p) = cache.get("frontend_admin_path") {
            if !p.trim().is_empty() {
                return p.clone();
            }
        }

        crc32b(app_key.as_bytes())
    }
}
