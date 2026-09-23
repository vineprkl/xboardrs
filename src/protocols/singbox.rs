use super::types::{ProxyContext, SubscriptionOutput};
use crate::entities::server::Model as ServerModel;
use serde_json::{json, Value};

const DEFAULT_SINGBOX_TEMPLATE: &str = r#"{
  "dns": {
    "servers": [
      { "address": "https://1.1.1.1/dns-query", "detour": "节点选择", "tag": "remote" },
      { "address": "https://223.5.5.5/dns-query", "detour": "direct", "tag": "local" }
    ],
    "strategy": "prefer_ipv4"
  },
  "inbounds": [
    { "type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": 2080 }
  ],
  "outbounds": [
    { "type": "selector", "tag": "节点选择", "outbounds": ["自动选择", "direct"] },
    { "type": "urltest", "tag": "自动选择", "outbounds": [], "url": "http://www.gstatic.com/generate_204", "interval": "3m" },
    { "type": "direct", "tag": "direct" },
    { "type": "block", "tag": "block" }
  ],
  "route": {
    "auto_detect_interface": true,
    "rules": [
      { "geosite": "cn", "outbound": "direct" },
      { "geoip": "cn", "outbound": "direct" },
      { "outbound": "节点选择" }
    ]
  }
}"#;

pub struct SingBoxProtocol;

impl SingBoxProtocol {
    pub fn handle(context: &ProxyContext, servers: &[ServerModel]) -> SubscriptionOutput {
        let mut root: Value = serde_json::from_str(DEFAULT_SINGBOX_TEMPLATE)
            .unwrap_or_else(|_| json!({ "outbounds": [] }));

        let mut outbounds = Vec::new();
        let mut tags = Vec::new();

        for server in servers {
            if let Some(outbound) = Self::build_outbound(context, server) {
                tags.push(server.name.clone());
                outbounds.push(outbound);
            }
        }

        // Inject outbounds
        if let Some(existing_outbounds) = root.get_mut("outbounds").and_then(|v| v.as_array_mut()) {
            // Inject tags into selector and urltest
            for ob in existing_outbounds.iter_mut() {
                if let Some(tag) = ob.get("tag").and_then(|v| v.as_str()) {
                    if tag == "节点选择" || tag == "自动选择" {
                        if let Some(ob_list) =
                            ob.get_mut("outbounds").and_then(|v| v.as_array_mut())
                        {
                            for t in &tags {
                                ob_list.push(Value::String(t.clone()));
                            }
                        }
                    }
                }
            }

            // Append all proxy outbounds
            existing_outbounds.extend(outbounds);
        }

        let content = serde_json::to_string_pretty(&root).unwrap_or_default();

        SubscriptionOutput {
            content,
            content_type: "application/json; charset=utf-8",
            filename: format!("{}.json", context.app_name),
            user_info: context.user_info_header(),
        }
    }

    pub fn build_outbound(context: &ProxyContext, server: &ServerModel) -> Option<Value> {
        let settings: Value = server
            .protocol_settings
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);

        let port_num: u16 = server.port.parse().unwrap_or(server.server_port as u16);

        match server.r#type.as_str() {
            "shadowsocks" | "ss" => {
                let cipher = settings
                    .get("cipher")
                    .and_then(|v| v.as_str())
                    .unwrap_or("aes-256-gcm");
                Some(json!({
                    "type": "shadowsocks",
                    "tag": server.name,
                    "server": server.host,
                    "server_port": port_num,
                    "method": cipher,
                    "password": context.user_uuid
                }))
            }
            "vmess" => {
                let mut ob = json!({
                    "type": "vmess",
                    "tag": server.name,
                    "server": server.host,
                    "server_port": port_num,
                    "uuid": context.user_uuid,
                    "security": "auto",
                    "alter_id": 0
                });
                if settings
                    .get("tls")
                    .map(|v| v == 1 || v == true)
                    .unwrap_or(false)
                {
                    let sni = settings
                        .get("tls_settings")
                        .and_then(|s| s.get("server_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&server.host);
                    ob["tls"] = json!({ "enabled": true, "server_name": sni });
                }
                Some(ob)
            }
            "vless" => {
                let mut ob = json!({
                    "type": "vless",
                    "tag": server.name,
                    "server": server.host,
                    "server_port": port_num,
                    "uuid": context.user_uuid
                });

                if let Some(flow) = settings.get("flow").and_then(|v| v.as_str()) {
                    if !flow.is_empty() {
                        ob["flow"] = Value::String(flow.to_string());
                    }
                }

                let tls_type = settings.get("tls").and_then(|v| v.as_i64()).unwrap_or(0);
                if tls_type == 1 {
                    let sni = settings
                        .get("tls_settings")
                        .and_then(|s| s.get("server_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&server.host);
                    ob["tls"] = json!({ "enabled": true, "server_name": sni });
                } else if tls_type == 2 {
                    let reality_opt = settings
                        .get("reality_settings")
                        .or_else(|| settings.get("tls_settings"));
                    if let Some(reality) = reality_opt {
                        let sni = reality
                            .get("server_name")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .unwrap_or(&server.host);
                        let pbk = reality
                            .get("public_key")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let sid = reality
                            .get("short_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let fp = reality
                            .get("fingerprint")
                            .and_then(|v| v.as_str())
                            .unwrap_or("chrome");

                        ob["tls"] = json!({
                            "enabled": true,
                            "server_name": sni,
                            "utls": {
                                "enabled": true,
                                "fingerprint": fp
                            },
                            "reality": {
                                "enabled": true,
                                "public_key": pbk,
                                "short_id": sid
                            }
                        });
                    }
                }
                Some(ob)
            }
            "trojan" => {
                let sni = settings
                    .get("tls_settings")
                    .and_then(|s| s.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&server.host);
                Some(json!({
                    "type": "trojan",
                    "tag": server.name,
                    "server": server.host,
                    "server_port": port_num,
                    "password": context.user_uuid,
                    "tls": { "enabled": true, "server_name": sni }
                }))
            }
            "hysteria" | "hysteria2" | "hy2" => {
                let sni = settings
                    .get("tls")
                    .and_then(|t| t.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&server.host);
                Some(json!({
                    "type": "hysteria2",
                    "tag": server.name,
                    "server": server.host,
                    "server_port": port_num,
                    "password": context.user_uuid,
                    "tls": { "enabled": true, "server_name": sni }
                }))
            }
            "tuic" => {
                let sni = settings
                    .get("tls")
                    .and_then(|t| t.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&server.host);
                Some(json!({
                    "type": "tuic",
                    "tag": server.name,
                    "server": server.host,
                    "server_port": port_num,
                    "uuid": context.user_uuid,
                    "password": context.user_uuid,
                    "congestion_controller": "bbr",
                    "tls": { "enabled": true, "server_name": sni }
                }))
            }
            _ => None,
        }
    }
}
