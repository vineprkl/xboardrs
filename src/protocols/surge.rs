use super::types::{ProxyContext, SubscriptionOutput};
use crate::entities::server::Model as ServerModel;
use serde_json::Value;

pub struct SurgeProtocol;

impl SurgeProtocol {
    pub fn handle(context: &ProxyContext, servers: &[ServerModel]) -> SubscriptionOutput {
        let mut proxy_lines = Vec::new();
        let mut proxy_names = Vec::new();

        for server in servers {
            if let Some(line) = Self::build_proxy_line(context, server) {
                proxy_names.push(server.name.clone());
                proxy_lines.push(line);
            }
        }

        let proxies_block = proxy_lines.join("\n");
        let proxy_names_joined = proxy_names.join(", ");

        let mut conf = String::new();
        conf.push_str("[General]\n");
        conf.push_str("loglevel = notify\n");
        conf.push_str("skip-proxy = 127.0.0.1, 192.168.0.0/16, 10.0.0.0/8, 172.16.0.0/12, 100.64.0.0/10, localhost, *.local\n\n");

        conf.push_str("[Proxy]\n");
        conf.push_str(&proxies_block);
        conf.push_str("\n\n");

        conf.push_str("[Proxy Group]\n");
        if !proxy_names.is_empty() {
            conf.push_str(&format!(
                "PROXY = select, AUTO, DIRECT, {}\n",
                proxy_names_joined
            ));
            conf.push_str(&format!(
                "AUTO = url-test, {}, url = http://www.gstatic.com/generate_204, interval = 300\n",
                proxy_names_joined
            ));
        } else {
            conf.push_str("PROXY = select, DIRECT\n");
        }
        conf.push('\n');

        conf.push_str("[Rule]\n");
        conf.push_str("GEOIP,CN,DIRECT\n");
        conf.push_str("FINAL,PROXY,dns-failed\n");

        SubscriptionOutput {
            content: conf,
            content_type: "text/plain; charset=utf-8",
            filename: format!("{}.conf", context.app_name),
            user_info: context.user_info_header(),
        }
    }

    pub fn build_proxy_line(context: &ProxyContext, server: &ServerModel) -> Option<String> {
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
                    "{} = ss, {}, {}, encrypt-method={}, password={}, udp-relay=true",
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
                    "{} = trojan, {}, {}, password={}, sni={}, udp-relay=true",
                    server.name, server.host, server.port, context.user_uuid, sni
                ))
            }
            "vmess" => {
                let mut line = format!(
                    "{} = vmess, {}, {}, username={}, udp-relay=true",
                    server.name, server.host, server.port, context.user_uuid
                );
                if settings
                    .get("tls")
                    .map(|v| v == 1 || v == true)
                    .unwrap_or(false)
                {
                    line.push_str(", tls=true");
                    if let Some(sni) = settings
                        .get("tls_settings")
                        .and_then(|s| s.get("server_name"))
                        .and_then(|v| v.as_str())
                    {
                        line.push_str(&format!(", sni={}", sni));
                    }
                }
                let net = settings
                    .get("network")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tcp");
                if net == "ws" {
                    line.push_str(", ws=true");
                    if let Some(path) = settings
                        .get("network_settings")
                        .and_then(|n| n.get("path"))
                        .and_then(|v| v.as_str())
                    {
                        line.push_str(&format!(", ws-path={}", path));
                    }
                }
                Some(line)
            }
            "hysteria" | "hysteria2" | "hy2" => {
                let sni = settings
                    .get("tls")
                    .and_then(|t| t.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&server.host);
                Some(format!(
                    "{} = hysteria2, {}, {}, password={}, sni={}, udp-relay=true",
                    server.name, server.host, server.port, context.user_uuid, sni
                ))
            }
            _ => None,
        }
    }
}
