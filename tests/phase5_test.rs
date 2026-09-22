use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
};
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, Schema, Set};
use serde_json::{json, Value};
use tower::ServiceExt;

use xboard_rs::{
    app_router_with_state,
    common::AppState,
    entities::{
        plan, server, server_group, user, Plan, Server, ServerGroup, ServerMachine,
        ServerMachineLoadHistory, ServerRoute, Setting, User,
    },
    services::{
        AuthService, DeviceStateService, PlanService, ServerService, SettingService, UserService,
    },
};

async fn setup_phase5_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory sqlite");

    let backend = db.get_database_backend();
    let schema = Schema::new(backend);

    let tables = vec![
        schema.create_table_from_entity(ServerGroup),
        schema.create_table_from_entity(Plan),
        schema.create_table_from_entity(Server),
        schema.create_table_from_entity(ServerMachine),
        schema.create_table_from_entity(ServerMachineLoadHistory),
        schema.create_table_from_entity(ServerRoute),
        schema.create_table_from_entity(User),
        schema.create_table_from_entity(Setting),
    ];

    for stmt in tables {
        db.execute(backend.build(&stmt))
            .await
            .expect("Failed to create table");
    }

    db
}

async fn create_test_state(db: DatabaseConnection) -> AppState {
    let setting_service = SettingService::new(db.clone());
    setting_service.load_all().await.unwrap();

    let device_state_service = DeviceStateService::new();
    let server_service = ServerService::new(
        db.clone(),
        setting_service.clone(),
        device_state_service.clone(),
    );
    let plan_service = PlanService::new(db.clone());
    let user_service = UserService::new();
    let auth_service = AuthService::new(db.clone(), setting_service.clone());

    AppState::new(
        db,
        setting_service,
        device_state_service,
        server_service,
        plan_service,
        user_service,
        auth_service,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_user(
    id: i32,
    email: &str,
    token: &str,
    uuid: &str,
    group_id: Option<i32>,
    plan_id: Option<i32>,
    transfer_enable: i64,
    u: i64,
    d: i64,
    banned: bool,
    expired_at: Option<i64>,
    now: i64,
) -> user::ActiveModel {
    user::ActiveModel {
        id: Set(id),
        email: Set(email.to_string()),
        password: Set("dummy_hash".to_string()),
        balance: Set(0),
        commission_type: Set(0),
        commission_balance: Set(0),
        t: Set(0),
        u: Set(u),
        d: Set(d),
        transfer_enable: Set(transfer_enable),
        banned: Set(banned),
        is_admin: Set(false),
        is_staff: Set(false),
        uuid: Set(uuid.to_string()),
        token: Set(token.to_string()),
        group_id: Set(group_id),
        plan_id: Set(plan_id),
        expired_at: Set(expired_at),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_client_subscribe_basic_and_protocols() {
    let db = setup_phase5_db().await;
    let state = create_test_state(db.clone()).await;

    let now = chrono::Utc::now().timestamp();

    // 1. Create server group
    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("VIP Group".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    // 2. Create Plan
    let p = plan::ActiveModel {
        id: Set(1),
        group_id: Set(1),
        transfer_enable: Set(100),
        name: Set("Plan A".to_string()),
        speed_limit: Set(Some(1000)),
        show: Set(true),
        sort: Set(Some(1)),
        renew: Set(true),
        sell: Set(Some(true)),
        content: Set(Some("Test Plan".to_string())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p.insert(&db).await.unwrap();

    // 3. Create User
    let u = build_user(
        1,
        "subscriber@test.com",
        "test_token_12345",
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        Some(1),
        Some(1),
        100_000_000_000, // 100 GB
        1_000_000_000,   // 1 GB
        2_000_000_000,   // 2 GB
        false,
        Some(now + 86400 * 30),
        now,
    );
    u.insert(&db).await.unwrap();

    // 4. Create Servers
    let s1 = server::ActiveModel {
        id: Set(1),
        group_ids: Set(Some("[1]".to_string())),
        name: Set("Hong Kong VLESS".to_string()),
        r#type: Set("vless".to_string()),
        host: Set("hk.example.com".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        show: Set(true),
        enabled: Set(Some(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    s1.insert(&db).await.unwrap();

    let s2 = server::ActiveModel {
        id: Set(2),
        group_ids: Set(Some("[1]".to_string())),
        name: Set("Tokyo SS".to_string()),
        r#type: Set("shadowsocks".to_string()),
        host: Set("jp.example.com".to_string()),
        port: Set("8388".to_string()),
        server_port: Set(8388),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        show: Set(true),
        enabled: Set(Some(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    s2.insert(&db).await.unwrap();

    let app = app_router_with_state(state.clone());

    // A. Path subscription /s/{token} with ClashMeta UA
    let req = Request::builder()
        .uri("/s/test_token_12345")
        .header("User-Agent", "Clash.Meta/1.14.0")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let headers = resp.headers();
    let userinfo = headers
        .get("subscription-userinfo")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(userinfo.contains("upload=1000000000; download=2000000000; total=100000000000;"));
    assert!(headers.get(header::CONTENT_DISPOSITION).is_some());

    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Hong Kong VLESS"));
    assert!(body_str.contains("proxies:"));

    // B. Legacy subscription /api/v1/client/subscribe?token=... with Sing-Box flag
    let req = Request::builder()
        .uri("/api/v1/client/subscribe?token=test_token_12345&flag=sing-box")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json_res: Value = serde_json::from_slice(&body).unwrap();
    assert!(json_res.get("outbounds").is_some());

    // C. Sub path /sub/{token} with Surge UA
    let req = Request::builder()
        .uri("/sub/test_token_12345")
        .header("User-Agent", "Surge/2589")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("[Proxy]"));

    // D. Path subscription with flag=shadowrocket
    let req = Request::builder()
        .uri("/s/test_token_12345?flag=shadowrocket")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    // Shadowrocket subscription is base64 encoded
    assert!(!body.is_empty());
}

#[tokio::test]
async fn test_client_subscribe_user_availability() {
    let db = setup_phase5_db().await;
    let state = create_test_state(db.clone()).await;

    let now = chrono::Utc::now().timestamp();

    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("Group 1".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    // 1. Banned User
    let u_banned = build_user(
        10,
        "banned@test.com",
        "token_banned",
        "u-1",
        Some(1),
        None,
        10_000_000_000,
        0,
        0,
        true, // banned
        Some(now + 3600),
        now,
    );
    u_banned.insert(&db).await.unwrap();

    // 2. Expired User
    let u_expired = build_user(
        11,
        "expired@test.com",
        "token_expired",
        "u-2",
        Some(1),
        None,
        10_000_000_000,
        0,
        0,
        false,
        Some(now - 3600), // expired
        now,
    );
    u_expired.insert(&db).await.unwrap();

    // 3. No Traffic User
    let u_notraffic = build_user(
        12,
        "notraffic@test.com",
        "token_notraffic",
        "u-3",
        Some(1),
        None,
        0, // 0 quota
        0,
        0,
        false,
        Some(now + 3600),
        now,
    );
    u_notraffic.insert(&db).await.unwrap();

    let app = app_router_with_state(state.clone());

    // A. Banned -> 403
    let req = Request::builder()
        .uri("/s/token_banned")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // B. Expired -> 403
    let req = Request::builder()
        .uri("/s/token_expired")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // C. No traffic -> 403
    let req = Request::builder()
        .uri("/s/token_notraffic")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // D. Invalid token -> 403
    let req = Request::builder()
        .uri("/s/token_non_existent")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // E. Missing token on legacy -> 403
    let req = Request::builder()
        .uri("/api/v1/client/subscribe")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_client_subscribe_filtering_and_info_nodes() {
    let db = setup_phase5_db().await;
    let state = create_test_state(db.clone()).await;

    let now = chrono::Utc::now().timestamp();

    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("Group 1".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    let mut u = build_user(
        1,
        "user@filter.test",
        "token_filter",
        "uuid-filter",
        Some(1),
        None,
        50_000_000_000,
        1_000_000_000,
        1_000_000_000,
        false,
        Some(now + 86400 * 10),
        now,
    );
    u.next_reset_at = Set(Some(now + 86400 * 5));
    u.insert(&db).await.unwrap();

    // 3 servers: 2 vless (HK, US), 1 shadowsocks (JP)
    let s1 = server::ActiveModel {
        id: Set(1),
        group_ids: Set(Some("[1]".to_string())),
        name: Set("HK 01".to_string()),
        r#type: Set("vless".to_string()),
        host: Set("hk.node".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        tags: Set(Some("HongKong,VIP".to_string())),
        show: Set(true),
        enabled: Set(Some(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    s1.insert(&db).await.unwrap();

    let s2 = server::ActiveModel {
        id: Set(2),
        group_ids: Set(Some("[1]".to_string())),
        name: Set("US 01".to_string()),
        r#type: Set("vless".to_string()),
        host: Set("us.node".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        tags: Set(Some("US".to_string())),
        show: Set(true),
        enabled: Set(Some(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    s2.insert(&db).await.unwrap();

    let s3 = server::ActiveModel {
        id: Set(3),
        group_ids: Set(Some("[1]".to_string())),
        name: Set("JP 01".to_string()),
        r#type: Set("shadowsocks".to_string()),
        host: Set("jp.node".to_string()),
        port: Set("8388".to_string()),
        server_port: Set(8388),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        tags: Set(Some("Japan".to_string())),
        show: Set(true),
        enabled: Set(Some(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    s3.insert(&db).await.unwrap();

    // Enable show_info_to_server_enable & show_protocol_to_server_enable
    state
        .setting_service
        .set("show_info_to_server_enable", "1")
        .await
        .unwrap();
    state
        .setting_service
        .set("show_protocol_to_server_enable", "1")
        .await
        .unwrap();

    let app = app_router_with_state(state.clone());

    // A. Filter by types=shadowsocks
    let req = Request::builder()
        .uri("/s/token_filter?types=shadowsocks&flag=clash")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);

    // Filtered out 2 lines notice
    assert!(body_str.contains("过滤掉2条线路"));
    // Info status nodes
    assert!(body_str.contains("剩余流量："));
    assert!(body_str.contains("套餐到期："));
    assert!(body_str.contains("距离下次重置剩余："));
    // Protocol prefix
    assert!(body_str.contains("[ss]JP 01"));
    assert!(!body_str.contains("US 01"));

    // B. Filter by keyword filter=HongKong
    let req = Request::builder()
        .uri("/s/token_filter?filter=HongKong&flag=clash")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);

    assert!(body_str.contains("[vless]HK 01"));
    assert!(!body_str.contains("JP 01"));
    assert!(!body_str.contains("US 01"));
}

#[tokio::test]
async fn test_client_app_version_and_config() {
    let db = setup_phase5_db().await;
    let state = create_test_state(db.clone()).await;

    let now = chrono::Utc::now().timestamp();

    // Set versions in setting_service
    state
        .setting_service
        .set("windows_version", "1.2.0")
        .await
        .unwrap();
    state
        .setting_service
        .set("windows_download_url", "https://dl.example.com/win.exe")
        .await
        .unwrap();
    state
        .setting_service
        .set("macos_version", "1.1.9")
        .await
        .unwrap();
    state
        .setting_service
        .set("macos_download_url", "https://dl.example.com/mac.dmg")
        .await
        .unwrap();
    state
        .setting_service
        .set("android_version", "1.0.5")
        .await
        .unwrap();
    state
        .setting_service
        .set("android_download_url", "https://dl.example.com/android.apk")
        .await
        .unwrap();

    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("Group 1".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    let u = build_user(
        1,
        "app_user@test.com",
        "token_app",
        "uuid-app",
        Some(1),
        None,
        10_000_000_000,
        0,
        0,
        false,
        Some(now + 86400),
        now,
    );
    u.insert(&db).await.unwrap();

    let app = app_router_with_state(state.clone());

    // 1. getVersion default
    let req = Request::builder()
        .uri("/api/v1/client/app/getVersion")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json_res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json_res["data"]["windows_version"], "1.2.0");
    assert_eq!(json_res["data"]["android_version"], "1.0.5");

    // 2. getVersion Tidalab Win64
    let req = Request::builder()
        .uri("/api/v1/client/app/getVersion")
        .header("User-Agent", "tidalab/4.0.0 Win64")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json_res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json_res["data"]["version"], "1.2.0");
    assert_eq!(
        json_res["data"]["download_url"],
        "https://dl.example.com/win.exe"
    );

    // 3. getConfig
    let req = Request::builder()
        .uri("/api/v1/client/app/getConfig?token=token_app")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("port: 7890"));
}

#[tokio::test]
async fn test_guest_comm_config() {
    let db = setup_phase5_db().await;
    let state = create_test_state(db.clone()).await;

    // Set configuration
    state
        .setting_service
        .set("tos_url", "https://example.com/terms")
        .await
        .unwrap();
    state
        .setting_service
        .set("email_verify", "1")
        .await
        .unwrap();
    state
        .setting_service
        .set("invite_force", "1")
        .await
        .unwrap();
    state
        .setting_service
        .set("email_whitelist_enable", "1")
        .await
        .unwrap();
    state
        .setting_service
        .set("email_whitelist_suffix", "gmail.com,outlook.com")
        .await
        .unwrap();
    state
        .setting_service
        .set("captcha_enable", "1")
        .await
        .unwrap();
    state
        .setting_service
        .set("captcha_type", "turnstile")
        .await
        .unwrap();
    state
        .setting_service
        .set("turnstile_site_key", "0x4AAAAAA")
        .await
        .unwrap();
    state
        .setting_service
        .set("app_description", "Best Board")
        .await
        .unwrap();

    let app = app_router_with_state(state.clone());

    let req = Request::builder()
        .uri("/api/v1/guest/comm/config")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json_res: Value = serde_json::from_slice(&body).unwrap();
    let data = &json_res["data"];

    assert_eq!(data["tos_url"], "https://example.com/terms");
    assert_eq!(data["is_email_verify"], 1);
    assert_eq!(data["is_invite_force"], 1);
    assert_eq!(
        data["email_whitelist_suffix"],
        json!(["gmail.com", "outlook.com"])
    );
    assert_eq!(data["is_captcha"], 1);
    assert_eq!(data["captcha_type"], "turnstile");
    assert_eq!(data["turnstile_site_key"], "0x4AAAAAA");
    assert_eq!(data["app_description"], "Best Board");
    assert_eq!(data["is_recaptcha"], 1);
}

#[tokio::test]
async fn test_guest_plan_fetch() {
    let db = setup_phase5_db().await;
    let state = create_test_state(db.clone()).await;

    let now = chrono::Utc::now().timestamp();

    // 0. Insert ServerGroup 1 for foreign key
    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("Group 1".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    // Plan 1: Active, sellable, visible
    let p1 = plan::ActiveModel {
        id: Set(1),
        group_id: Set(1),
        transfer_enable: Set(100),
        name: Set("Standard Plan".to_string()),
        speed_limit: Set(Some(500)),
        device_limit: Set(Some(5)),
        show: Set(true),
        sort: Set(Some(1)),
        renew: Set(true),
        sell: Set(Some(true)),
        content: Set(Some("Speed: {{speed}} Mbps, Traffic: {{transfer}} GB, Reset: {{reset_method}}, Devices: {{devices}}".to_string())),
        month_price: Set(Some(1500)),
        year_price: Set(Some(15000)),
        reset_traffic_method: Set(Some(1)), // Monthly
        capacity_limit: Set(Some(100)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p1.insert(&db).await.unwrap();

    // Plan 2: Hidden (show = false)
    let p2 = plan::ActiveModel {
        id: Set(2),
        group_id: Set(1),
        transfer_enable: Set(200),
        name: Set("Hidden Plan".to_string()),
        show: Set(false),
        sort: Set(Some(2)),
        renew: Set(true),
        sell: Set(Some(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p2.insert(&db).await.unwrap();

    // Plan 3: Not for sale (sell = false)
    let p3 = plan::ActiveModel {
        id: Set(3),
        group_id: Set(1),
        transfer_enable: Set(300),
        name: Set("Unsellable Plan".to_string()),
        show: Set(true),
        sort: Set(Some(3)),
        renew: Set(true),
        sell: Set(Some(false)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p3.insert(&db).await.unwrap();

    // Plan 4: Sold out (capacity_limit = 1, and 1 active user)
    let p4 = plan::ActiveModel {
        id: Set(4),
        group_id: Set(1),
        transfer_enable: Set(500),
        name: Set("Sold Out Plan".to_string()),
        show: Set(true),
        sort: Set(Some(4)),
        renew: Set(true),
        sell: Set(Some(true)),
        capacity_limit: Set(Some(1)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p4.insert(&db).await.unwrap();

    let u_occupy = build_user(
        99,
        "occupy@test.com",
        "token_occupy",
        "uuid-occupy",
        Some(1),
        Some(4),
        10_000_000_000,
        0,
        0,
        false,
        Some(now + 86400),
        now,
    );
    u_occupy.insert(&db).await.unwrap();

    let app = app_router_with_state(state.clone());

    let req = Request::builder()
        .uri("/api/v1/guest/plan/fetch")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json_res: Value = serde_json::from_slice(&body).unwrap();
    let list = json_res["data"].as_array().unwrap();

    // Only Plan 1 should be present!
    assert_eq!(list.len(), 1);
    let plan_data = &list[0];
    assert_eq!(plan_data["id"], 1);
    assert_eq!(plan_data["name"], "Standard Plan");
    assert_eq!(plan_data["month_price"], 1500);
    assert_eq!(plan_data["year_price"], 15000);

    // Verify placeholder replacements
    let content = plan_data["content"].as_str().unwrap();
    assert!(content.contains("Speed: 500 Mbps"));
    assert!(content.contains("Traffic: 100 GB"));
    assert!(content.contains("Reset: Monthly"));
    assert!(content.contains("Devices: 5"));
}

#[tokio::test]
async fn test_dynamic_subscribe_path() {
    let db = setup_phase5_db().await;
    let state = create_test_state(db.clone()).await;

    let now = chrono::Utc::now().timestamp();

    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("Group 1".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    let u = build_user(
        1,
        "custom@sub.test",
        "token_custom_sub",
        "uuid-custom",
        Some(1),
        None,
        10_000_000_000,
        0,
        0,
        false,
        Some(now + 86400),
        now,
    );
    u.insert(&db).await.unwrap();

    let s1 = server::ActiveModel {
        id: Set(1),
        group_ids: Set(Some("[1]".to_string())),
        name: Set("Node 1".to_string()),
        r#type: Set("vless".to_string()),
        host: Set("node1.test".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        show: Set(true),
        enabled: Set(Some(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    s1.insert(&db).await.unwrap();

    // Set custom subscribe_path
    state
        .setting_service
        .set("subscribe_path", "mysub")
        .await
        .unwrap();

    let app = app_router_with_state(state.clone());

    // A. Request custom path /mysub/token_custom_sub -> should succeed via dynamic_fallback
    let req = Request::builder()
        .uri("/mysub/token_custom_sub?flag=clash")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Node 1"));

    // B. Request unknown route -> 404
    let req = Request::builder()
        .uri("/random_route/does_not_exist")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
