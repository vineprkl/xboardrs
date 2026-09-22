use super::types::{ProxyContext, SubscriptionOutput};
use crate::entities::server::Model as ServerModel;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde_json::Value;

pub struct GeneralProtocol;

impl GeneralProtocol {
    pub fn handle(context: &ProxyContext, servers: &[ServerModel]) -> SubscriptionOutput {
        let mut lines = Vec::new();

        for server in servers {
            if let Some(uri) = Self::build_server_uri(context, server) {
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

    pub fn build_server_uri(context: &ProxyContext, server: &ServerModel) -> Option<String> {
        let settings: Value = server
            .protocol_settings
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);

        match server.r#type.as_str() {
            "shadowsocks" | "ss" => Some(Self::build_shadowsocks(context, server, &settings)),
            "vmess" => Some(Self::build_vmess(context, server, &settings)),
            "vless" => Some(Self::build_vless(context, server, &settings)),
            "trojan" => Some(Self::build_trojan(context, server, &settings)),
            "hysteria" | "hysteria2" | "hy2" => {
                Some(Self::build_hysteria(context, server, &settings))
            }
            "tuic" => Some(Self::build_tuic(context, server, &settings)),
            _ => None,
        }
    }

    fn build_shadowsocks(context: &ProxyContext, server: &ServerModel, settings: &Value) -> String {
        let cipher = settings
            .get("cipher")
            .and_then(|v| v.as_str())
            .unwrap_or("aes-256-gcm");
        let password = &context.user_uuid;
        let plain = format!("{}:{}", cipher, password);
        let encoded_auth = BASE64.encode(plain.as_bytes());
        let name = urlencoding::encode(&server.name);

        format!(
            "ss://{}@{}:{}#{}",
            encoded_auth, server.host, server.port, name
        )
    }

    fn build_vmess(context: &ProxyContext, server: &ServerModel, settings: &Value) -> String {
        let net = settings
            .get("network")
            .and_then(|v| v.as_str())
            .unwrap_or("tcp");
        let tls = if settings
            .get("tls")
            .map(|v| v == 1 || v == true)
            .unwrap_or(false)
        {
            "tls"
        } else {
            ""
        };
        let sni = settings
            .get("tls_settings")
            .and_then(|t| t.get("server_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = settings
            .get("network_settings")
            .and_then(|n| n.get("path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let host = settings
            .get("network_settings")
            .and_then(|n| n.get("headers"))
            .and_then(|h| h.get("Host"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let vmess_json = serde_json::json!({
            "v": "2",
            "ps": server.name,
            "add": server.host,
            "port": server.port,
            "id": context.user_uuid,
            "aid": "0",
            "net": net,
            "type": "none",
            "host": host,
            "path": path,
            "tls": tls,
            "sni": sni,
        });

        let encoded = BASE64.encode(vmess_json.to_string().as_bytes());
        format!("vmess://{}", encoded)
    }

    fn build_vless(context: &ProxyContext, server: &ServerModel, settings: &Value) -> String {
        let mut query = Vec::new();
        let net = settings
            .get("network")
            .and_then(|v| v.as_str())
            .unwrap_or("tcp");
        query.push(format!("type={}", net));

        let flow = settings.get("flow").and_then(|v| v.as_str());
        if let Some(f) = flow {
            if !f.is_empty() {
                query.push(format!("flow={}", f));
            }
        }

        let tls_type = settings.get("tls").and_then(|v| v.as_i64()).unwrap_or(0);
        if tls_type == 1 {
            query.push("security=tls".to_string());
            if let Some(sni) = settings
                .get("tls_settings")
                .and_then(|s| s.get("server_name"))
                .and_then(|v| v.as_str())
            {
                query.push(format!("sni={}", sni));
            }
        } else if tls_type == 2 {
            query.push("security=reality".to_string());
            if let Some(reality) = settings.get("reality_settings") {
                if let Some(pbk) = reality.get("public_key").and_then(|v| v.as_str()) {
                    query.push(format!("pbk={}", pbk));
                }
                if let Some(sid) = reality.get("short_id").and_then(|v| v.as_str()) {
                    query.push(format!("sid={}", sid));
                }
                if let Some(sni) = reality.get("server_name").and_then(|v| v.as_str()) {
                    query.push(format!("sni={}", sni));
                }
            }
        }

        let query_str = if query.is_empty() {
            String::new()
        } else {
            format!("?{}", query.join("&"))
        };

        let name = urlencoding::encode(&server.name);
        format!(
            "vless://{}@{}:{}{}#{}",
            context.user_uuid, server.host, server.port, query_str, name
        )
    }

    fn build_trojan(context: &ProxyContext, server: &ServerModel, settings: &Value) -> String {
        let mut query = Vec::new();
        query.push("security=tls".to_string());
        if let Some(sni) = settings
            .get("tls_settings")
            .and_then(|s| s.get("server_name"))
            .and_then(|v| v.as_str())
        {
            query.push(format!("sni={}", sni));
        }

        let query_str = format!("?{}", query.join("&"));
        let name = urlencoding::encode(&server.name);
        format!(
            "trojan://{}@{}:{}{}#{}",
            context.user_uuid, server.host, server.port, query_str, name
        )
    }

    fn build_hysteria(context: &ProxyContext, server: &ServerModel, settings: &Value) -> String {
        let mut query = Vec::new();
        let sni = settings
            .get("tls")
            .and_then(|t| t.get("server_name"))
            .and_then(|v| v.as_str())
            .unwrap_or(&server.host);
        query.push(format!("sni={}", sni));

        if let Some(insecure) = settings
            .get("tls")
            .and_then(|t| t.get("allow_insecure"))
            .and_then(|v| v.as_bool())
        {
            if insecure {
                query.push("insecure=1".to_string());
            }
        }

        let query_str = format!("?{}", query.join("&"));
        let name = urlencoding::encode(&server.name);
        format!(
            "hy2://{}@{}:{}{}#{}",
            context.user_uuid, server.host, server.port, query_str, name
        )
    }

    fn build_tuic(context: &ProxyContext, server: &ServerModel, settings: &Value) -> String {
        let mut query = Vec::new();
        let sni = settings
            .get("tls")
            .and_then(|t| t.get("server_name"))
            .and_then(|v| v.as_str())
            .unwrap_or(&server.host);
        query.push(format!("sni={}", sni));
        query.push("congestion_control=bbr".to_string());

        let query_str = format!("?{}", query.join("&"));
        let name = urlencoding::encode(&server.name);
        format!(
            "tuic://{}:{}@{}:{}{}#{}",
            context.user_uuid, context.user_uuid, server.host, server.port, query_str, name
        )
    }
}
