use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyContext {
    pub user_uuid: String,
    pub user_token: String,
    pub user_email: String,
    pub app_name: String,
    pub upload_bytes: i64,
    pub download_bytes: i64,
    pub total_bytes: i64,
    pub expired_at: i64,
    pub host: Option<String>,
}

impl ProxyContext {
    pub fn user_info_header(&self) -> String {
        format!(
            "upload={}; download={}; total={}; expire={}",
            self.upload_bytes, self.download_bytes, self.total_bytes, self.expired_at
        )
    }
}

#[derive(Debug, Clone)]
pub struct SubscriptionOutput {
    pub content: String,
    pub content_type: &'static str,
    pub filename: String,
    pub user_info: String,
}
