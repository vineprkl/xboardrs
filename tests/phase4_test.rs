use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait, Schema, Set,
};
use serde_json::{json, Value};
use tower::ServiceExt;

use xboard_rs::{
    app_router_with_state,
    common::AppState,
    entities::{
        server, server_group, server_machine, setting,
        stat::{server as stat_server, user as stat_user},
        user, Plan, Server, ServerGroup, ServerMachine, ServerMachineLoadHistory, ServerRoute,
        Setting, User,
    },
    services::{
        AuthService, DeviceStateService, PlanService, ServerService, SettingService, UserService,
    },
};

async fn setup_phase4_db() -> DatabaseConnection {
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
        schema.create_table_from_entity(stat_user::Entity),
        schema.create_table_from_entity(stat_server::Entity),
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

#[tokio::test]
async fn test_node_auth_success_and_failure() {
    let db = setup_phase4_db().await;
    let now = chrono::Utc::now().timestamp();

    // 1. Configure server_token in setting
    setting::ActiveModel {
        name: Set("server_token".to_string()),
        value: Set(Some("secret_server_token_123".to_string())),
        updated_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // 2. Insert Node
    let node = server::ActiveModel {
        name: Set("US Test Node".to_string()),
        r#type: Set("vless".to_string()),
        code: Set(Some("us-vless".to_string())),
        host: Set("us.example.com".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        show: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let state = create_test_state(db).await;
    let app = app_router_with_state(state);

    // Test 2a: Valid token and node_id -> 200 OK
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/server/UniProxy/config?token=secret_server_token_123&node_id={}",
                    node.id
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Test 2b: Invalid token -> 401 Unauthorized
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/server/UniProxy/config?token=wrong_token&node_id={}",
                    node.id
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Test 2c: Missing node_id -> 400 Bad Request
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/server/UniProxy/config?token=secret_server_token_123")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Test 2d: Non-existent node_id -> 404 Not Found
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/server/UniProxy/config?token=secret_server_token_123&node_id=99999")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_machine_auth_success_and_restrictions() {
    let db = setup_phase4_db().await;
    let now = chrono::Utc::now().timestamp();

    // 1. Create Machine
    let machine = server_machine::ActiveModel {
        name: Set("Dedicated Host 1".to_string()),
        token: Set("machine_token_abc".to_string()),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // 2. Create Node linked to Machine
    let node = server::ActiveModel {
        name: Set("HK Machine Node".to_string()),
        r#type: Set("trojan".to_string()),
        machine_id: Set(Some(machine.id)),
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
    }
    .insert(&db)
    .await
    .unwrap();

    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state);

    // Test: Machine token auth succeeds
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v2/server/config?token=machine_token_abc&machine_id={}&node_id={}",
                    machine.id, node.id
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Test: Disabled machine returns 403 Forbidden
    let machine_id = machine.id;
    let mut m_active: server_machine::ActiveModel = machine.into();
    m_active.is_active = Set(false);
    m_active.update(&db).await.unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v2/server/config?token=machine_token_abc&machine_id={}&node_id={}",
                    machine_id, node.id
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_uniproxy_config_and_etag_caching() {
    let db = setup_phase4_db().await;
    let now = chrono::Utc::now().timestamp();

    setting::ActiveModel {
        name: Set("server_token".to_string()),
        value: Set(Some("token123".to_string())),
        updated_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let node = server::ActiveModel {
        name: Set("SS 2022 Node".to_string()),
        r#type: Set("shadowsocks".to_string()),
        host: Set("ss.example.com".to_string()),
        port: Set("8388".to_string()),
        server_port: Set(8388),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        protocol_settings: Set(Some(
            json!({
                "cipher": "2022-blake3-aes-128-gcm",
                "plugin": "v2ray-plugin",
                "plugin_opts": "server"
            })
            .to_string(),
        )),
        show: Set(true),
        created_at: Set(1700000000),
        updated_at: Set(1700000000),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let state = create_test_state(db).await;
    let app = app_router_with_state(state);

    // 1. Initial Request -> 200 OK + ETag
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/server/UniProxy/config?token=token123&node_id={}",
                    node.id
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let etag_header = resp
        .headers()
        .get(header::ETAG)
        .expect("ETag header missing")
        .to_str()
        .unwrap()
        .to_string();

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(val["protocol"], "shadowsocks");
    assert_eq!(val["cipher"], "2022-blake3-aes-128-gcm");
    assert!(val["server_key"].as_str().is_some());
    assert_eq!(val["base_config"]["push_interval"], 60);

    // 2. Second Request with If-None-Match -> 304 Not Modified
    let resp_304 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/server/UniProxy/config?token=token123&node_id={}",
                    node.id
                ))
                .header("If-None-Match", etag_header)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp_304.status(), StatusCode::NOT_MODIFIED);
}

#[allow(clippy::too_many_arguments)]
fn create_test_user(
    email: &str,
    uuid: &str,
    token: &str,
    group_id: Option<i32>,
    transfer_enable: i64,
    u: i64,
    d: i64,
    banned: bool,
    expired_at: Option<i64>,
    device_limit: Option<i32>,
    now: i64,
) -> user::ActiveModel {
    user::ActiveModel {
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
        expired_at: Set(expired_at),
        device_limit: Set(device_limit),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_uniproxy_user_filtering_and_etag() {
    let db = setup_phase4_db().await;
    let now = chrono::Utc::now().timestamp();

    setting::ActiveModel {
        name: Set("server_token".to_string()),
        value: Set(Some("token123".to_string())),
        updated_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    server_group::ActiveModel {
        id: Set(1),
        name: Set("Group 1".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    server_group::ActiveModel {
        id: Set(99),
        name: Set("Group 99".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // Node is assigned to group [1, 2]
    let node = server::ActiveModel {
        name: Set("Group 1 Node".to_string()),
        r#type: Set("vless".to_string()),
        group_ids: Set(Some("[1, 2]".to_string())),
        host: Set("vless.example.com".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        show: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // 1. User Valid: group 1, active, has quota
    let u_valid = create_test_user(
        "valid@example.com",
        "uuid-valid",
        "token-valid",
        Some(1),
        100 * 1024 * 1024,
        10 * 1024,
        10 * 1024,
        false,
        Some(now + 86400),
        None,
        now,
    )
    .insert(&db)
    .await
    .unwrap();

    // 2. User Banned: group 1, banned = true
    create_test_user(
        "banned@example.com",
        "uuid-banned",
        "token-banned",
        Some(1),
        100 * 1024 * 1024,
        0,
        0,
        true,
        None,
        None,
        now,
    )
    .insert(&db)
    .await
    .unwrap();

    // 3. User Expired: expired_at < now
    create_test_user(
        "expired@example.com",
        "uuid-expired",
        "token-expired",
        Some(1),
        100 * 1024 * 1024,
        0,
        0,
        false,
        Some(now - 100),
        None,
        now,
    )
    .insert(&db)
    .await
    .unwrap();

    // 4. User Quota Exhausted: u + d >= transfer_enable
    create_test_user(
        "exhausted@example.com",
        "uuid-exhausted",
        "token-exhausted",
        Some(1),
        1000,
        500,
        500,
        false,
        None,
        None,
        now,
    )
    .insert(&db)
    .await
    .unwrap();

    // 5. User Different Group: group_id = 99
    create_test_user(
        "diffgroup@example.com",
        "uuid-diff",
        "token-diff",
        Some(99),
        100 * 1024 * 1024,
        0,
        0,
        false,
        None,
        None,
        now,
    )
    .insert(&db)
    .await
    .unwrap();

    let state = create_test_state(db).await;
    let app = app_router_with_state(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/server/UniProxy/user?token=token123&node_id={}",
                    node.id
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val: Value = serde_json::from_slice(&body).unwrap();

    let users = val["users"].as_array().unwrap();
    // Only u_valid should be present
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["id"], u_valid.id);
    assert_eq!(users[0]["uuid"], "uuid-valid");

    // Check that touch_node updated last_check_at
    let last_check = state.server_service.get_last_check_at(node.id).await;
    assert!(last_check.is_some());
}

#[tokio::test]
async fn test_uniproxy_traffic_push_with_multiplier() {
    let db = setup_phase4_db().await;
    let now = chrono::Utc::now().timestamp();

    setting::ActiveModel {
        name: Set("server_token".to_string()),
        value: Set(Some("token123".to_string())),
        updated_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Node rate is 2.0x
    let node = server::ActiveModel {
        name: Set("2x Rate Node".to_string()),
        r#type: Set("vmess".to_string()),
        host: Set("vmess.example.com".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(2.0),
        rate_time_enable: Set(false),
        u: Set(Some(0)),
        d: Set(Some(0)),
        show: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let user_model = create_test_user(
        "user_traffic@example.com",
        "uuid-traffic",
        "token-traffic",
        None,
        1000000,
        100,
        200,
        false,
        None,
        None,
        now,
    )
    .insert(&db)
    .await
    .unwrap();

    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state);

    // Push: user reported 1000 bytes upload, 4000 bytes download
    let traffic_payload = json!({
        user_model.id.to_string(): [1000, 4000]
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/server/UniProxy/push?token=token123&node_id={}",
                    node.id
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(traffic_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(val["status"], "success");
    assert_eq!(val["data"], true);

    // Verify user traffic incremented by rate 2.0x
    let updated_user = User::find_by_id(user_model.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    // u: 100 + (1000 * 2) = 2100
    // d: 200 + (4000 * 2) = 8200
    assert_eq!(updated_user.u, 2100);
    assert_eq!(updated_user.d, 8200);

    // Verify server raw traffic incremented without rate (1000, 4000)
    let updated_server = Server::find_by_id(node.id).one(&db).await.unwrap().unwrap();
    assert_eq!(updated_server.u, Some(1000));
    assert_eq!(updated_server.d, Some(4000));

    // Verify StatUser and StatServer rows exist
    let user_stats = stat_user::Entity::find().all(&db).await.unwrap();
    assert_eq!(user_stats.len(), 1);
    assert_eq!(user_stats[0].u, 2000);
    assert_eq!(user_stats[0].d, 8000);
    assert_eq!(user_stats[0].server_rate, 2.0);

    let server_stats = stat_server::Entity::find().all(&db).await.unwrap();
    assert_eq!(server_stats.len(), 1);
    assert_eq!(server_stats[0].u, 1000);
    assert_eq!(server_stats[0].d, 4000);
}

#[tokio::test]
async fn test_uniproxy_alive_and_alivelist_workflow() {
    let db = setup_phase4_db().await;
    let now = chrono::Utc::now().timestamp();

    setting::ActiveModel {
        name: Set("server_token".to_string()),
        value: Set(Some("token123".to_string())),
        updated_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let node = server::ActiveModel {
        name: Set("Alive Test Node".to_string()),
        r#type: Set("trojan".to_string()),
        group_ids: Set(Some("[1]".to_string())),
        host: Set("trojan.example.com".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        show: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    server_group::ActiveModel {
        id: Set(1),
        name: Set("Group 1".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    let user1 = create_test_user(
        "u1@example.com",
        "u1-uuid",
        "u1-token",
        Some(1),
        1000000,
        0,
        0,
        false,
        None,
        Some(2),
        now,
    )
    .insert(&db)
    .await
    .unwrap();

    let state = create_test_state(db).await;
    let app = app_router_with_state(state);

    // 1. Post alive IPs: user1 reports two IPs, one with port
    let alive_payload = json!({
        user1.id.to_string(): ["10.0.0.1:1234", "10.0.0.1:5678", "10.0.0.2:9999"]
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/server/UniProxy/alive?token=token123&node_id={}",
                    node.id
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(alive_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Query alivelist: normalized unique IPs = 10.0.0.1 and 10.0.0.2 => count 2
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/server/UniProxy/alivelist?token=token123&node_id={}",
                    node.id
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(val["alive"][user1.id.to_string()], 2);
}

#[tokio::test]
async fn test_v2_server_handshake_and_aggregated_report() {
    let db = setup_phase4_db().await;
    let now = chrono::Utc::now().timestamp();

    setting::ActiveModel {
        name: Set("server_token".to_string()),
        value: Set(Some("token123".to_string())),
        updated_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    setting::ActiveModel {
        name: Set("server_ws_enable".to_string()),
        value: Set(Some("1".to_string())),
        updated_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let node = server::ActiveModel {
        name: Set("V2 Aggregate Node".to_string()),
        r#type: Set("vless".to_string()),
        host: Set("v2.example.com".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        show: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let state = create_test_state(db).await;
    let app = app_router_with_state(state);

    // 1. Handshake
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/server/handshake?token=token123")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(val["websocket"]["enabled"], true);

    // 2. Aggregated Report
    let report_payload = json!({
        "status": {
            "cpu": 12.5,
            "mem": { "total": 1000, "used": 400 },
            "disk": { "total": 5000, "used": 2000 }
        }
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v2/server/report?token=token123&node_id={}",
                    node.id
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(report_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(val["data"], true);
}

#[tokio::test]
async fn test_v2_machine_nodes_and_status_recording() {
    let db = setup_phase4_db().await;
    let now = chrono::Utc::now().timestamp();

    let machine = server_machine::ActiveModel {
        name: Set("Cluster Node 1".to_string()),
        token: Set("mach_token_999".to_string()),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Node 1: attached to machine and enabled
    server::ActiveModel {
        name: Set("Managed Node 1".to_string()),
        r#type: Set("vless".to_string()),
        machine_id: Set(Some(machine.id)),
        host: Set("m1.example.com".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        show: Set(true),
        enabled: Set(Some(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state);

    // 1. Fetch nodes for machine
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v2/server/machine/nodes?token=mach_token_999&machine_id={}",
                    machine.id
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(val["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(val["nodes"][0]["name"], "Managed Node 1");

    // 2. Report machine status
    let status_payload = json!({
        "cpu": 25.0,
        "mem": { "total": 8000000, "used": 3000000 },
        "disk": { "total": 50000000, "used": 15000000 },
        "net": { "in_speed": 120.5, "out_speed": 450.0 }
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v2/server/machine/status?token=mach_token_999&machine_id={}",
                    machine.id
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(status_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    // Verify history row was inserted
    let history = ServerMachineLoadHistory::find().all(&db).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].machine_id, machine.id);
    assert_eq!(history[0].cpu, 25.0);
    assert_eq!(history[0].net_in_speed, Some(120.5));
    assert_eq!(history[0].net_out_speed, Some(450.0));

    // Verify machine last_seen_at was updated
    let updated_machine = ServerMachine::find_by_id(machine.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(updated_machine.last_seen_at.is_some());
    assert!(updated_machine.load_status.is_some());
}
