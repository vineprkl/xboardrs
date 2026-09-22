use super::types::{ProxyContext, SubscriptionOutput};
use crate::entities::server::Model as ServerModel;
use serde_json::Value;

pub struct LoonProtocol;

impl LoonProtocol {
    pub fn handle(context: &ProxyContext, servers: &[ServerModel]) -> SubscriptionOutput {
        let mut lines = Vec::new();

        for server in servers {
            if let Some(line) = Self::build_server_line(context, server) {
                lines.push(line);
            }
        }

        let content = lines.join("\n");

        SubscriptionOutput {
            content,
            content_type: "text/plain; charset=utf-8",
            filename: format!("{}.conf", context.app_name),
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
                    "{} = Shadowsocks, {}, {}, \"{}\", \"{}\"",
                    server.name, server.host, server.port, cipher, context.user_uuid
                ))
            }
            "trojan" => {
                let sni = settings
                    .get("tls_settings")
                    .and_then(|s| s.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&server.host);
                Some(format!(
                    "{} = Trojan, {}, {}, \"{}\", tls-name=\"{}\"",
                    server.name, server.host, server.port, context.user_uuid, sni
                ))
            }
            "vmess" => Some(format!(
                "{} = VMess, {}, {}, auto, \"{}\"",
                server.name, server.host, server.port, context.user_uuid
            )),
            "hysteria" | "hysteria2" | "hy2" => {
                let sni = settings
                    .get("tls")
                    .and_then(|t| t.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&server.host);
                Some(format!(
                    "{} = Hysteria2, {}, {}, \"{}\", sni=\"{}\"",
                    server.name, server.host, server.port, context.user_uuid, sni
                ))
            }
            _ => None,
        }
    }
}
