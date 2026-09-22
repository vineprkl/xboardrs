use super::types::{ProxyContext, SubscriptionOutput};
use crate::entities::server::Model as ServerModel;
use serde_json::Value;

const DEFAULT_CLASH_TEMPLATE: &str = r#"
mixed-port: 7890
allow-lan: true
mode: rule
log-level: info
unified-delay: true
tcp-concurrent: true

dns:
  enable: true
  ipv6: false
  enhanced-mode: fake-ip
  fake-ip-range: 198.18.0.1/16
  nameserver:
    - https://doh.pub/dns-query
    - https://dns.alidns.com/dns-query

proxies: []

proxy-groups:
  - name: 节点选择
    type: select
    proxies:
      - 自动选择
      - DIRECT
  - name: 自动选择
    type: url-test
    url: http://www.gstatic.com/generate_204
    interval: 300
    tolerance: 50
    proxies: []

rules:
  - GEOIP,LAN,DIRECT
  - GEOIP,CN,DIRECT
  - MATCH,节点选择
"#;

pub struct ClashMetaProtocol;

impl ClashMetaProtocol {
    pub fn handle(context: &ProxyContext, servers: &[ServerModel]) -> SubscriptionOutput {
        let mut yaml_val: serde_yaml::Value = serde_yaml::from_str(DEFAULT_CLASH_TEMPLATE)
            .unwrap_or(serde_yaml::Value::Mapping(Default::default()));

        let mut proxies_list = Vec::new();
        let mut proxy_names = Vec::new();

        for server in servers {
            if let Some(proxy_val) = Self::build_proxy(context, server) {
                proxy_names.push(server.name.clone());
                proxies_list.push(proxy_val);
            }
        }

        // 1. Assign to `proxies`
        if let Some(proxies_entry) = yaml_val.get_mut("proxies") {
            let existing = proxies_entry.as_sequence().cloned().unwrap_or_default();
            let mut combined = existing;
            combined.extend(proxies_list);
            *proxies_entry = serde_yaml::Value::Sequence(combined);
        }

        // 2. Inject names into `proxy-groups`
        if let Some(groups) = yaml_val.get_mut("proxy-groups") {
            if let Some(groups_seq) = groups.as_sequence_mut() {
                for group in groups_seq.iter_mut() {
                    if let Some(group_map) = group.as_mapping_mut() {
                        let proxies_key = serde_yaml::Value::String("proxies".to_string());
                        let mut curr_proxies = group_map
                            .get(&proxies_key)
                            .and_then(|v| v.as_sequence().cloned())
                            .unwrap_or_default();

                        for name in &proxy_names {
                            curr_proxies.push(serde_yaml::Value::String(name.clone()));
                        }
                        group_map.insert(proxies_key, serde_yaml::Value::Sequence(curr_proxies));
                    }
                }
            }
        }

        // 3. Optional direct rule for subscription domain
        if let Some(host) = &context.host {
            if let Some(rules) = yaml_val.get_mut("rules") {
                if let Some(rules_seq) = rules.as_sequence_mut() {
                    rules_seq.insert(
                        0,
                        serde_yaml::Value::String(format!("DOMAIN,{},DIRECT", host)),
                    );
                }
            }
        }

        let output_str =
            serde_yaml::to_string(&yaml_val).unwrap_or_else(|_| DEFAULT_CLASH_TEMPLATE.to_string());

        SubscriptionOutput {
            content: output_str,
            content_type: "text/yaml; charset=utf-8",
            filename: format!("{}.yaml", context.app_name),
            user_info: context.user_info_header(),
        }
    }

    pub fn build_proxy(context: &ProxyContext, server: &ServerModel) -> Option<serde_yaml::Value> {
        let settings: Value = server
            .protocol_settings
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);

        let mut map = serde_json::Map::new();
        map.insert("name".into(), Value::String(server.name.clone()));
        map.insert("server".into(), Value::String(server.host.clone()));
        let port_num: u16 = server.port.parse().unwrap_or(server.server_port as u16);
        map.insert("port".into(), Value::Number(port_num.into()));

        match server.r#type.as_str() {
            "shadowsocks" | "ss" => {
                map.insert("type".into(), Value::String("ss".into()));
                let cipher = settings
                    .get("cipher")
                    .and_then(|v| v.as_str())
                    .unwrap_or("aes-256-gcm");
                map.insert("cipher".into(), Value::String(cipher.into()));
                map.insert("password".into(), Value::String(context.user_uuid.clone()));
                map.insert("udp".into(), Value::Bool(true));
            }
            "vmess" => {
                map.insert("type".into(), Value::String("vmess".into()));
                map.insert("uuid".into(), Value::String(context.user_uuid.clone()));
                map.insert("alterId".into(), Value::Number(0.into()));
                map.insert("cipher".into(), Value::String("auto".into()));
                map.insert("udp".into(), Value::Bool(true));
                if settings
                    .get("tls")
                    .map(|v| v == 1 || v == true)
                    .unwrap_or(false)
                {
                    map.insert("tls".into(), Value::Bool(true));
                    if let Some(sni) = settings
                        .get("tls_settings")
                        .and_then(|s| s.get("server_name"))
                        .and_then(|v| v.as_str())
                    {
                        map.insert("servername".into(), Value::String(sni.into()));
                    }
                }
                let net = settings
                    .get("network")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tcp");
                map.insert("network".into(), Value::String(net.into()));
            }
            "vless" => {
                map.insert("type".into(), Value::String("vless".into()));
                map.insert("uuid".into(), Value::String(context.user_uuid.clone()));
                map.insert("udp".into(), Value::Bool(true));

                if let Some(flow) = settings.get("flow").and_then(|v| v.as_str()) {
                    if !flow.is_empty() {
                        map.insert("flow".into(), Value::String(flow.into()));
                    }
                }

                let tls_type = settings.get("tls").and_then(|v| v.as_i64()).unwrap_or(0);
                if tls_type == 1 {
                    map.insert("tls".into(), Value::Bool(true));
                    if let Some(sni) = settings
                        .get("tls_settings")
                        .and_then(|s| s.get("server_name"))
                        .and_then(|v| v.as_str())
                    {
                        map.insert("servername".into(), Value::String(sni.into()));
                    }
                } else if tls_type == 2 {
                    map.insert("tls".into(), Value::Bool(true));
                    if let Some(reality) = settings.get("reality_settings") {
                        if let Some(sni) = reality.get("server_name").and_then(|v| v.as_str()) {
                            map.insert("servername".into(), Value::String(sni.into()));
                        }
                        let mut reality_opts = serde_json::Map::new();
                        if let Some(pbk) = reality.get("public_key").and_then(|v| v.as_str()) {
                            reality_opts.insert("public-key".into(), Value::String(pbk.into()));
                        }
                        if let Some(sid) = reality.get("short_id").and_then(|v| v.as_str()) {
                            reality_opts.insert("short-id".into(), Value::String(sid.into()));
                        }
                        map.insert("reality-opts".into(), Value::Object(reality_opts));
                    }
                }
            }
            "trojan" => {
                map.insert("type".into(), Value::String("trojan".into()));
                map.insert("password".into(), Value::String(context.user_uuid.clone()));
                map.insert("udp".into(), Value::Bool(true));
                if let Some(sni) = settings
                    .get("tls_settings")
                    .and_then(|s| s.get("server_name"))
                    .and_then(|v| v.as_str())
                {
                    map.insert("sni".into(), Value::String(sni.into()));
                }
            }
            "hysteria" | "hysteria2" | "hy2" => {
                map.insert("type".into(), Value::String("hysteria2".into()));
                map.insert("password".into(), Value::String(context.user_uuid.clone()));
                let sni = settings
                    .get("tls")
                    .and_then(|t| t.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&server.host);
                map.insert("sni".into(), Value::String(sni.into()));
                if let Some(insecure) = settings
                    .get("tls")
                    .and_then(|t| t.get("allow_insecure"))
                    .and_then(|v| v.as_bool())
                {
                    map.insert("skip-cert-verify".into(), Value::Bool(insecure));
                }
            }
            "tuic" => {
                map.insert("type".into(), Value::String("tuic".into()));
                map.insert("uuid".into(), Value::String(context.user_uuid.clone()));
                map.insert("password".into(), Value::String(context.user_uuid.clone()));
                let sni = settings
                    .get("tls")
                    .and_then(|t| t.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&server.host);
                map.insert("sni".into(), Value::String(sni.into()));
                map.insert("congestion-controller".into(), Value::String("bbr".into()));
            }
            _ => return None,
        }

        serde_yaml::to_value(Value::Object(map)).ok()
    }
}
