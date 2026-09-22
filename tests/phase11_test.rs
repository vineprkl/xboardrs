use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
};
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, Schema, Set};
use std::path::Path;
use tower::ServiceExt;

use xboard_rs::{
    app_router_with_state,
    common::AppState,
    entities::{
        admin_audit_log::Entity as AdminAuditLog,
        commission_log::Entity as CommissionLog,
        coupon::Entity as Coupon,
        gift_card::{GiftCardCode, GiftCardTemplate, GiftCardUsage},
        invite_code::Entity as InviteCode,
        knowledge::Entity as Knowledge,
        notice::Entity as Notice,
        order::Entity as Order,
        payment::Entity as Payment,
        personal_access_token::Entity as PersonalAccessToken,
        plan::{ActiveModel as PlanActiveModel, Entity as Plan},
        server::Entity as Server,
        server_group::{ActiveModel as ServerGroupActiveModel, Entity as ServerGroup},
        server_machine::Entity as ServerMachine,
        server_machine_load_history::Entity as ServerMachineLoadHistory,
        server_route::Entity as ServerRoute,
        setting::Entity as Setting,
        stat::{Stat, StatServer, StatUser},
        ticket::Entity as Ticket,
        ticket_message::Entity as TicketMessage,
        traffic_reset_log::Entity as TrafficResetLog,
        user::Entity as User,
    },
    handlers::web::mime_for_path,
    services::{
        AuthService, DeviceStateService, PlanService, ServerService, SettingService, UserService,
    },
    utils::crc32b,
};

async fn setup_phase11_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory sqlite");

    db.execute_unprepared("PRAGMA foreign_keys = OFF;")
        .await
        .expect("Failed to disable foreign keys");

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
        schema.create_table_from_entity(InviteCode),
        schema.create_table_from_entity(PersonalAccessToken),
        schema.create_table_from_entity(Notice),
        schema.create_table_from_entity(Knowledge),
        schema.create_table_from_entity(Coupon),
        schema.create_table_from_entity(Ticket),
        schema.create_table_from_entity(TicketMessage),
        schema.create_table_from_entity(GiftCardTemplate),
        schema.create_table_from_entity(GiftCardCode),
        schema.create_table_from_entity(GiftCardUsage),
        schema.create_table_from_entity(CommissionLog),
        schema.create_table_from_entity(TrafficResetLog),
        schema.create_table_from_entity(Order),
        schema.create_table_from_entity(Payment),
        schema.create_table_from_entity(Stat),
        schema.create_table_from_entity(StatUser),
        schema.create_table_from_entity(StatServer),
        schema.create_table_from_entity(AdminAuditLog),
    ];

    for stmt in tables {
        db.execute(backend.build(&stmt))
            .await
            .expect("Failed to create table");
    }

    let now = chrono::Utc::now().timestamp();
    let grp = ServerGroupActiveModel {
        id: Set(1),
        name: Set("Default Group".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    let p = PlanActiveModel {
        id: Set(1),
        group_id: Set(1),
        transfer_enable: Set(100),
        name: Set("Default Plan".to_string()),
        show: Set(true),
        renew: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p.insert(&db).await.unwrap();

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
async fn test_user_dashboard_rendering() {
    let db = setup_phase11_db().await;
    let state = create_test_state(db).await;
    let app = app_router_with_state(state);

    let req = Request::builder()
        .uri("/")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("<title>Xboard</title>"));
    assert!(html.contains(r#"window.routerBase = "/";"#));
    assert!(html.contains("window.settings = {"));
    assert!(html.contains(r#"title: 'Xboard'"#));
    assert!(html.contains(r#"<div id="app"></div>"#));
}

#[tokio::test]
async fn test_user_dashboard_safe_mode_allowed_and_blocked() {
    let db = setup_phase11_db().await;
    let state = create_test_state(db).await;

    // Enable safe mode with allowed domain
    state
        .setting_service
        .set("safe_mode_enable", "1")
        .await
        .unwrap();
    state
        .setting_service
        .set("app_url", "https://board.example.com")
        .await
        .unwrap();

    let app = app_router_with_state(state);

    // 1. Request with mismatched host should be blocked (403 Forbidden)
    let req_blocked = Request::builder()
        .uri("/")
        .method("GET")
        .header(header::HOST, "malicious.com")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_blocked = app.clone().oneshot(req_blocked).await.unwrap();
    assert_eq!(resp_blocked.status(), StatusCode::FORBIDDEN);

    // 2. Request with matching host should be allowed (200 OK)
    let req_allowed = Request::builder()
        .uri("/")
        .method("GET")
        .header(header::HOST, "board.example.com")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_allowed = app.oneshot(req_allowed).await.unwrap();
    assert_eq!(resp_allowed.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_user_spa_route_fallback() {
    let db = setup_phase11_db().await;
    let state = create_test_state(db).await;
    let app = app_router_with_state(state);

    let spa_routes = vec![
        "/login",
        "/register",
        "/dashboard",
        "/plan",
        "/order/20240919001",
        "/ticket",
        "/knowledge",
        "/profile",
    ];

    for route in spa_routes {
        let req = Request::builder()
            .uri(route)
            .method("GET")
            .body(axum::body::Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );

        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains(r#"<div id="app"></div>"#));
    }
}

#[tokio::test]
async fn test_admin_dashboard_rendering_and_secure_path() {
    let db = setup_phase11_db().await;
    let state = create_test_state(db).await;

    state
        .setting_service
        .set("secure_path", "secret_admin_entrance")
        .await
        .unwrap();

    let app = app_router_with_state(state);

    // 1. Direct secure entrance
    let req = Request::builder()
        .uri("/secret_admin_entrance")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("<title>XBoard</title>"));
    assert!(html.contains("window.settings = {"));
    assert!(html.contains(r#"secure_path: "secret_admin_entrance""#));
    assert!(html.contains(r#"<div id="root"></div>"#));
    assert!(html.contains("/assets/admin/"));
    assert!(html.contains("/assets/admin/locales/"));

    // 2. Admin sub-route (SPA routing in admin dashboard)
    let req_sub = Request::builder()
        .uri("/secret_admin_entrance/user")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp_sub = app.oneshot(req_sub).await.unwrap();
    assert_eq!(resp_sub.status(), StatusCode::OK);
    let body_sub = to_bytes(resp_sub.into_body(), usize::MAX).await.unwrap();
    let html_sub = String::from_utf8(body_sub.to_vec()).unwrap();
    assert!(html_sub.contains(r#"<div id="root"></div>"#));
}

#[tokio::test]
async fn test_admin_dashboard_fallback_to_crc32b_app_key() {
    let db = setup_phase11_db().await;
    let state = create_test_state(db).await;

    let app_key = std::env::var("APP_KEY")
        .unwrap_or_else(|_| "base64:xboard_default_key_32bytes!!".to_string());
    let expected_crc = crc32b(app_key.as_bytes());

    let app = app_router_with_state(state);

    let req = Request::builder()
        .uri(format!("/{}", expected_crc))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains(r#"<div id="root"></div>"#));
    assert!(html.contains(&format!(r#"secure_path: "{}""#, expected_crc)));
}

#[tokio::test]
async fn test_api_not_found_returns_404_not_html() {
    let db = setup_phase11_db().await;
    let state = create_test_state(db).await;
    let app = app_router_with_state(state);

    // GET nonexistent API endpoint
    let req_get = Request::builder()
        .uri("/api/v1/user/nonexistent_action")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_get = app.clone().oneshot(req_get).await.unwrap();
    assert_eq!(resp_get.status(), StatusCode::NOT_FOUND);

    // POST nonexistent API endpoint
    let req_post = Request::builder()
        .uri("/api/v2/admin/nonexistent_action")
        .method("POST")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_post = app.oneshot(req_post).await.unwrap();
    assert_eq!(resp_post.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_robots_txt_and_static_file_behavior() {
    let db = setup_phase11_db().await;
    let state = create_test_state(db).await;
    let app = app_router_with_state(state);

    // 1. Robots.txt
    let req_robots = Request::builder()
        .uri("/robots.txt")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_robots = app.clone().oneshot(req_robots).await.unwrap();
    assert_eq!(resp_robots.status(), StatusCode::OK);
    assert_eq!(
        resp_robots.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/plain; charset=utf-8"
    );
    let body_robots = to_bytes(resp_robots.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8(body_robots.to_vec())
        .unwrap()
        .contains("User-agent: *"));

    // 2. Real theme static asset (umi.js) should return 200 OK with javascript MIME
    let req_umi = Request::builder()
        .uri("/theme/Xboard/assets/umi.js")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_umi = app.clone().oneshot(req_umi).await.unwrap();
    assert_eq!(resp_umi.status(), StatusCode::OK);
    assert_eq!(
        resp_umi.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/javascript; charset=utf-8"
    );
    let body_umi = to_bytes(resp_umi.into_body(), usize::MAX).await.unwrap();
    assert!(!body_umi.is_empty());

    // 3. Real admin static asset (manifest.json) should return 200 OK with json MIME
    let req_manifest = Request::builder()
        .uri("/assets/admin/manifest.json")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_manifest = app.clone().oneshot(req_manifest).await.unwrap();
    assert_eq!(resp_manifest.status(), StatusCode::OK);
    assert_eq!(
        resp_manifest.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    // 4. Non-existent static asset should return 404 (not HTML dashboard)
    let req_asset = Request::builder()
        .uri("/assets/admin/nonexistent_bundle_123.js")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_asset = app.clone().oneshot(req_asset).await.unwrap();
    assert_eq!(resp_asset.status(), StatusCode::NOT_FOUND);

    // 5. Non-existent theme asset should return 404 (not HTML dashboard)
    let req_theme = Request::builder()
        .uri("/theme/Xboard/assets/nonexistent_image.png")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_theme = app.oneshot(req_theme).await.unwrap();
    assert_eq!(resp_theme.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_mime_type_resolution() {
    assert_eq!(
        mime_for_path(Path::new("app.js")),
        "application/javascript; charset=utf-8"
    );
    assert_eq!(
        mime_for_path(Path::new("index.css")),
        "text/css; charset=utf-8"
    );
    assert_eq!(
        mime_for_path(Path::new("index.html")),
        "text/html; charset=utf-8"
    );
    assert_eq!(mime_for_path(Path::new("logo.png")), "image/png");
    assert_eq!(mime_for_path(Path::new("icon.svg")), "image/svg+xml");
    assert_eq!(mime_for_path(Path::new("font.woff2")), "font/woff2");
    assert_eq!(
        mime_for_path(Path::new("manifest.json")),
        "application/json"
    );
}

#[tokio::test]
async fn test_headless_api_fallback_when_theme_missing() {
    let db = setup_phase11_db().await;
    let state = create_test_state(db).await;

    // Set theme to a nonexistent theme name
    state
        .setting_service
        .set("frontend_theme", "NonExistentThemeXYZ_999")
        .await
        .unwrap();

    let app = app_router_with_state(state);

    let req = Request::builder()
        .uri("/")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["mode"], "headless_api");
    assert_eq!(json["data"]["status"], "online");
}
