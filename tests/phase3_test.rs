use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde_json::Value;
use xboard_rs::{
    entities::server::Model as ServerModel,
    protocols::{detect_client, generate_subscription, ClientType, ProxyContext},
};

fn mock_test_servers() -> Vec<ServerModel> {
    vec![
        // 1. VLESS Reality Node
        ServerModel {
            id: 1,
            name: "HK VLESS Reality".to_string(),
            r#type: "vless".to_string(),
            code: Some("hk-vless".to_string()),
            parent_id: None,
            machine_id: None,
            group_ids: Some("[1]".to_string()),
            route_ids: None,
            tags: Some(r#"["HK"]"#.to_string()),
            host: "hk.example.com".to_string(),
            port: "443".to_string(),
            server_port: 443,
            rate: 1.0,
            rate_time_enable: false,
            rate_time_ranges: None,
            protocol_settings: Some(
                serde_json::json!({
                    "network": "tcp",
                    "flow": "xtls-rprx-vision",
                    "tls": 2, // Reality
                    "reality_settings": {
                        "server_name": "gateway.icloud.com",
                        "public_key": "abc_pub_key_1234567890abcdef",
                        "short_id": "12345678"
                    }
                })
                .to_string(),
            ),
            custom_outbounds: None,
            custom_routes: None,
            cert_config: None,
            show: true,
            enabled: Some(true),
            sort: Some(1),
            transfer_enable: None,
            u: None,
            d: None,
            created_at: 1700000000,
            updated_at: 1700000000,
        },
        // 2. VMess WebSocket Node
        ServerModel {
            id: 2,
            name: "JP VMess WS".to_string(),
            r#type: "vmess".to_string(),
            code: Some("jp-vmess".to_string()),
            parent_id: None,
            machine_id: None,
            group_ids: Some("[1]".to_string()),
            route_ids: None,
            tags: Some(r#"["JP"]"#.to_string()),
            host: "jp.example.com".to_string(),
            port: "443".to_string(),
            server_port: 443,
            rate: 1.0,
            rate_time_enable: false,
            rate_time_ranges: None,
            protocol_settings: Some(
                serde_json::json!({
                    "network": "ws",
                    "tls": 1,
                    "tls_settings": {
                        "server_name": "jp.example.com"
                    },
                    "network_settings": {
                        "path": "/v2ray-ws"
                    }
                })
                .to_string(),
            ),
            custom_outbounds: None,
            custom_routes: None,
            cert_config: None,
            show: true,
            enabled: Some(true),
            sort: Some(2),
            transfer_enable: None,
            u: None,
            d: None,
            created_at: 1700000000,
            updated_at: 1700000000,
        },
        // 3. Trojan Node
        ServerModel {
            id: 3,
            name: "SG Trojan".to_string(),
            r#type: "trojan".to_string(),
            code: Some("sg-trojan".to_string()),
            parent_id: None,
            machine_id: None,
            group_ids: Some("[1]".to_string()),
            route_ids: None,
            tags: Some(r#"["SG"]"#.to_string()),
            host: "sg.example.com".to_string(),
            port: "443".to_string(),
            server_port: 443,
            rate: 1.0,
            rate_time_enable: false,
            rate_time_ranges: None,
            protocol_settings: Some(
                serde_json::json!({
                    "tls_settings": {
                        "server_name": "sg.example.com"
                    }
                })
                .to_string(),
            ),
            custom_outbounds: None,
            custom_routes: None,
            cert_config: None,
            show: true,
            enabled: Some(true),
            sort: Some(3),
            transfer_enable: None,
            u: None,
            d: None,
            created_at: 1700000000,
            updated_at: 1700000000,
        },
        // 4. Shadowsocks Node
        ServerModel {
            id: 4,
            name: "US Shadowsocks".to_string(),
            r#type: "shadowsocks".to_string(),
            code: Some("us-ss".to_string()),
            parent_id: None,
            machine_id: None,
            group_ids: Some("[1]".to_string()),
            route_ids: None,
            tags: Some(r#"["US"]"#.to_string()),
            host: "us.example.com".to_string(),
            port: "8388".to_string(),
            server_port: 8388,
            rate: 0.5,
            rate_time_enable: false,
            rate_time_ranges: None,
            protocol_settings: Some(
                serde_json::json!({
                    "cipher": "chacha20-ietf-poly1305"
                })
                .to_string(),
            ),
            custom_outbounds: None,
            custom_routes: None,
            cert_config: None,
            show: true,
            enabled: Some(true),
            sort: Some(4),
            transfer_enable: None,
            u: None,
            d: None,
            created_at: 1700000000,
            updated_at: 1700000000,
        },
        // 5. Hysteria2 Node
        ServerModel {
            id: 5,
            name: "KR Hysteria2".to_string(),
            r#type: "hysteria2".to_string(),
            code: Some("kr-hy2".to_string()),
            parent_id: None,
            machine_id: None,
            group_ids: Some("[1]".to_string()),
            route_ids: None,
            tags: Some(r#"["KR"]"#.to_string()),
            host: "kr.example.com".to_string(),
            port: "8443".to_string(),
            server_port: 8443,
            rate: 2.0,
            rate_time_enable: false,
            rate_time_ranges: None,
            protocol_settings: Some(
                serde_json::json!({
                    "tls": {
                        "server_name": "kr.example.com",
                        "allow_insecure": false
                    }
                })
                .to_string(),
            ),
            custom_outbounds: None,
            custom_routes: None,
            cert_config: None,
            show: true,
            enabled: Some(true),
            sort: Some(5),
            transfer_enable: None,
            u: None,
            d: None,
            created_at: 1700000000,
            updated_at: 1700000000,
        },
    ]
}

fn mock_proxy_context() -> ProxyContext {
    ProxyContext {
        user_uuid: "e82d7c54-6e47-49d6-953e-32408bcf77cb".to_string(),
        user_token: "test_sub_token_abcdef1234567890".to_string(),
        user_email: "test@xboard.local".to_string(),
        app_name: "XboardPro".to_string(),
        upload_bytes: 1024 * 1024 * 500,        // 500 MB
        download_bytes: 1024 * 1024 * 1024 * 5, // 5 GB
        total_bytes: 1024 * 1024 * 1024 * 100,  // 100 GB
        expired_at: 1750000000,
        host: Some("sub.xboard.local".to_string()),
    }
}

#[test]
fn test_client_detection() {
    // 1. By flag parameter
    assert_eq!(detect_client(Some("meta"), None), ClientType::ClashMeta);
    assert_eq!(detect_client(Some("verge"), None), ClientType::ClashMeta);
    assert_eq!(detect_client(Some("clash"), None), ClientType::Clash);
    assert_eq!(detect_client(Some("sing-box"), None), ClientType::SingBox);
    assert_eq!(detect_client(Some("surge"), None), ClientType::Surge);
    assert_eq!(
        detect_client(Some("shadowrocket"), None),
        ClientType::Shadowrocket
    );
    assert_eq!(
        detect_client(Some("quantumult-x"), None),
        ClientType::QuantumultX
    );
    assert_eq!(detect_client(Some("loon"), None), ClientType::Loon);
    assert_eq!(detect_client(Some("v2rayn"), None), ClientType::General);

    // 2. By User-Agent header
    assert_eq!(
        detect_client(None, Some("Clash-Verge/1.3.8")),
        ClientType::ClashMeta
    );
    assert_eq!(
        detect_client(None, Some("Mihomo/1.18.7")),
        ClientType::ClashMeta
    );
    assert_eq!(
        detect_client(None, Some("sing-box 1.9.0")),
        ClientType::SingBox
    );
    assert_eq!(
        detect_client(None, Some("Surge/2610 (iPhone; iOS 17.5)")),
        ClientType::Surge
    );
    assert_eq!(
        detect_client(None, Some("Shadowrocket/1993 CFNetwork")),
        ClientType::Shadowrocket
    );
    assert_eq!(
        detect_client(None, Some("Quantumult X/1.0.30")),
        ClientType::QuantumultX
    );
    assert_eq!(
        detect_client(None, Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")),
        ClientType::General
    );
}

#[test]
fn test_general_protocol_generation() {
    let servers = mock_test_servers();
    let ctx = mock_proxy_context();

    let output = generate_subscription(ClientType::General, &ctx, &servers);
    assert_eq!(output.content_type, "text/plain; charset=utf-8");
    assert!(output.user_info.contains("upload="));

    // Decode base64
    let decoded_bytes = BASE64
        .decode(&output.content)
        .expect("Base64 decode failed");
    let decoded_str = String::from_utf8(decoded_bytes).expect("UTF-8 decode failed");

    // Must contain all 5 protocols
    assert!(decoded_str.contains("vless://"));
    assert!(decoded_str.contains("security=reality"));
    assert!(decoded_str.contains("pbk=abc_pub_key_1234567890abcdef"));
    assert!(decoded_str.contains("vmess://"));
    assert!(decoded_str.contains("trojan://"));
    assert!(decoded_str.contains("ss://"));
    assert!(decoded_str.contains("hy2://"));
}

#[test]
fn test_clash_meta_yaml_generation() {
    let servers = mock_test_servers();
    let ctx = mock_proxy_context();

    let output = generate_subscription(ClientType::ClashMeta, &ctx, &servers);
    assert_eq!(output.content_type, "text/yaml; charset=utf-8");

    // Parse output as YAML to ensure syntactical correctness
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&output.content).expect("Generated YAML is invalid!");

    let proxies = yaml
        .get("proxies")
        .and_then(|v| v.as_sequence())
        .expect("Missing proxies section");
    assert_eq!(proxies.len(), 5);

    // Verify VLESS Reality node properties
    let vless = &proxies[0];
    assert_eq!(vless["type"].as_str(), Some("vless"));
    assert_eq!(vless["flow"].as_str(), Some("xtls-rprx-vision"));
    assert_eq!(vless["tls"].as_bool(), Some(true));
    assert_eq!(
        vless["reality-opts"]["public-key"].as_str(),
        Some("abc_pub_key_1234567890abcdef")
    );

    // Verify proxy-groups have node names injected
    let groups = yaml
        .get("proxy-groups")
        .and_then(|v| v.as_sequence())
        .expect("Missing proxy-groups section");
    let select_group = &groups[0];
    let group_proxies = select_group["proxies"]
        .as_sequence()
        .expect("Missing group proxies");
    assert!(group_proxies
        .iter()
        .any(|p| p.as_str() == Some("HK VLESS Reality")));
}

#[test]
fn test_sing_box_json_generation() {
    let servers = mock_test_servers();
    let ctx = mock_proxy_context();

    let output = generate_subscription(ClientType::SingBox, &ctx, &servers);
    assert_eq!(output.content_type, "application/json; charset=utf-8");

    // Parse output as JSON
    let json: Value =
        serde_json::from_str(&output.content).expect("Generated SingBox JSON is invalid!");

    let outbounds = json["outbounds"]
        .as_array()
        .expect("Missing outbounds array");

    // Find vless outbound
    let vless_ob = outbounds
        .iter()
        .find(|o| o["tag"] == "HK VLESS Reality")
        .expect("Missing HK VLESS Reality outbound");
    assert_eq!(vless_ob["type"], "vless");
    assert_eq!(vless_ob["flow"], "xtls-rprx-vision");
    assert_eq!(vless_ob["tls"]["reality"]["enabled"], true);
    assert_eq!(
        vless_ob["tls"]["reality"]["public_key"],
        "abc_pub_key_1234567890abcdef"
    );

    // Find hysteria2 outbound
    let hy2_ob = outbounds
        .iter()
        .find(|o| o["tag"] == "KR Hysteria2")
        .expect("Missing KR Hysteria2 outbound");
    assert_eq!(hy2_ob["type"], "hysteria2");
    assert_eq!(hy2_ob["tls"]["enabled"], true);

    // Verify selector outbound has node tags
    let selector = outbounds
        .iter()
        .find(|o| o["tag"] == "节点选择")
        .expect("Missing 节点选择 selector");
    let selector_outbounds = selector["outbounds"].as_array().unwrap();
    assert!(selector_outbounds.iter().any(|t| t == "HK VLESS Reality"));
    assert!(selector_outbounds.iter().any(|t| t == "KR Hysteria2"));
}

#[test]
fn test_surge_configuration_generation() {
    let servers = mock_test_servers();
    let ctx = mock_proxy_context();

    let output = generate_subscription(ClientType::Surge, &ctx, &servers);
    assert_eq!(output.content_type, "text/plain; charset=utf-8");

    assert!(output.content.contains("[Proxy]"));
    assert!(output.content.contains("[Proxy Group]"));
    assert!(output.content.contains("[Rule]"));
    assert!(output
        .content
        .contains("US Shadowsocks = ss, us.example.com, 8388"));
    assert!(output
        .content
        .contains("SG Trojan = trojan, sg.example.com, 443"));
    assert!(output
        .content
        .contains("KR Hysteria2 = hysteria2, kr.example.com, 8443"));
    assert!(output.content.contains("PROXY = select, AUTO, DIRECT, "));
}

#[test]
fn test_shadowrocket_and_quantumult_x_and_loon() {
    let servers = mock_test_servers();
    let ctx = mock_proxy_context();

    // 1. Shadowrocket
    let sr = generate_subscription(ClientType::Shadowrocket, &ctx, &servers);
    let sr_raw = String::from_utf8(BASE64.decode(&sr.content).unwrap()).unwrap();
    assert!(sr_raw.contains("STATUS=🚀"));
    assert!(sr_raw.contains("vless://"));
    assert!(sr_raw.contains("hy2://"));

    // 2. Quantumult X
    let qx = generate_subscription(ClientType::QuantumultX, &ctx, &servers);
    let qx_raw = String::from_utf8(BASE64.decode(&qx.content).unwrap()).unwrap();
    assert!(qx_raw.contains("shadowsocks="));
    assert!(qx_raw.contains("trojan="));
    assert!(qx_raw.contains("vmess="));
    assert!(qx_raw.contains("vless="));

    // 3. Loon
    let loon = generate_subscription(ClientType::Loon, &ctx, &servers);
    assert!(loon.content.contains("Shadowsocks"));
    assert!(loon.content.contains("Trojan"));
    assert!(loon.content.contains("VMess"));
    assert!(loon.content.contains("Hysteria2"));
}
