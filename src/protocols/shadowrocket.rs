use super::{
    general::GeneralProtocol,
    types::{ProxyContext, SubscriptionOutput},
};
use crate::entities::server::Model as ServerModel;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

pub struct ShadowrocketProtocol;

impl ShadowrocketProtocol {
    pub fn handle(context: &ProxyContext, servers: &[ServerModel]) -> SubscriptionOutput {
        let mut lines = Vec::new();

        // Status header line
        let upload_gb = (context.upload_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
        let download_gb = (context.download_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
        let total_gb = (context.total_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
        let expire_str = if context.expired_at == 0 {
            "N/A".to_string()
        } else {
            chrono::DateTime::from_timestamp(context.expired_at, 0)
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "N/A".to_string())
        };

        lines.push(format!(
            "STATUS=🚀↑:{:.2}GB,↓:{:.2}GB,TOT:{:.2}GB💡Expires:{}",
            upload_gb, download_gb, total_gb, expire_str
        ));

        for server in servers {
            if let Some(uri) = GeneralProtocol::build_server_uri(context, server) {
                lines.push(uri);
            }
        }

        let combined = lines.join("\r\n");
        let encoded = BASE64.encode(combined.as_bytes());

        SubscriptionOutput {
            content: encoded,
            content_type: "text/plain; charset=utf-8",
            filename: context.app_name.clone(),
            user_info: context.user_info_header(),
        }
    }
}
