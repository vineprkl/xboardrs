use super::types::{ProxyContext, SubscriptionOutput};
use crate::entities::server::Model as ServerModel;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde_json::Value;

pub struct QuantumultXProtocol;

impl QuantumultXProtocol {
    pub fn handle(context: &ProxyContext, servers: &[ServerModel]) -> SubscriptionOutput {
        let mut lines = Vec::new();

        for server in servers {
            if let Some(line) = Self::build_server_line(context, server) {
                lines.push(line);
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

    pub fn build_server_line(context: &ProxyContext, server: &ServerModel) -> Option<String> {
        let settings: Value = server
            .protocol_settings
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);

        match server.r#type.as_str() {
            "shadowsocks" | "ss" => {
                let cipher = settings
                    .get("cipher")
                    .and_then(|v| v.as_str())
                    .unwrap_or("aes-256-gcm");
                Some(format!(
                    "shadowsocks={}:{}, method={}, password={}, tag={}",
                    server.host, server.port, cipher, context.user_uuid, server.name
                ))
            }
            "trojan" => {
                let sni = settings
                    .get("tls_settings")
                    .and_then(|s| s.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&server.host);
                Some(format!(
                    "trojan={}:{}, password={}, over-tls=true, tls-host={}, tag={}",
                    server.host, server.port, context.user_uuid, sni, server.name
                ))
            }
            "vmess" => {
                let mut line = format!(
                    "vmess={}:{}, method=none, password={}",
                    server.host, server.port, context.user_uuid
                );
                if settings
                    .get("tls")
                    .map(|v| v == 1 || v == true)
                    .unwrap_or(false)
                {
                    line.push_str(", over-tls=true");
                    if let Some(sni) = settings
                        .get("tls_settings")
                        .and_then(|s| s.get("server_name"))
                        .and_then(|v| v.as_str())
                    {
                        line.push_str(&format!(", tls-host={}", sni));
                    }
                }
                line.push_str(&format!(", tag={}", server.name));
                Some(line)
            }
            "vless" => {
                let mut line = format!(
                    "vless={}:{}, method=none, password={}",
                    server.host, server.port, context.user_uuid
                );
                let tls_type = settings.get("tls").and_then(|v| v.as_i64()).unwrap_or(0);
                if tls_type == 1 {
                    line.push_str(", over-tls=true");
                }
                line.push_str(&format!(", tag={}", server.name));
                Some(line)
            }
            _ => None,
        }
    }
}
