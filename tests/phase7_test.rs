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
        coupon, gift_card, knowledge, notice, personal_access_token, plan, server, server_group,
        user, CommissionLog, Coupon, GiftCardCode, GiftCardTemplate, GiftCardUsage, InviteCode,
        Knowledge, Notice, Order, Payment, PersonalAccessToken, Plan, Server, ServerGroup,
        ServerMachine, ServerMachineLoadHistory, ServerRoute, Setting, StatUser, Ticket,
        TicketMessage, User,
    },
    services::{
        AuthService, DeviceStateService, PlanService, ServerService, SettingService, UserService,
    },
    utils::{hash_password, sha256_hex, verify_password},
};

async fn setup_phase7_db() -> DatabaseConnection {
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
        schema.create_table_from_entity(Order),
        schema.create_table_from_entity(Payment),
        schema.create_table_from_entity(StatUser),
    ];

    for stmt in tables {
        db.execute(backend.build(&stmt))
            .await
            .expect("Failed to create table");
    }

    let now = chrono::Utc::now().timestamp();
    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("VIP Group".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

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

async fn insert_user_with_token(
    db: &DatabaseConnection,
    id: i32,
    email: &str,
    password_plain: &str,
    is_admin: bool,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let hashed = hash_password(password_plain).unwrap();

    let u = user::ActiveModel {
        id: Set(id),
        email: Set(email.to_string()),
        password: Set(hashed),
        balance: Set(1000), // 10.00
        commission_type: Set(0),
        commission_balance: Set(5000), // 50.00
        t: Set(0),
        u: Set(1024),
        d: Set(2048),
        transfer_enable: Set(10737418240), // 10 GB
        banned: Set(false),
        is_admin: Set(is_admin),
        is_staff: Set(false),
        uuid: Set(format!("uuid-{}", id)),
        token: Set(format!("token-{}", id)),
        group_id: Set(Some(1)),
        plan_id: Set(Some(1)),
        expired_at: Set(Some(now + 86400 * 30)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    u.insert(db).await.unwrap();

    let plain_token = format!("test_plain_token_for_user_{}", id);
    let token_hash = sha256_hex(plain_token.as_bytes());

    let pat = personal_access_token::ActiveModel {
        tokenable_type: Set("App\\Models\\User".to_string()),
        tokenable_id: Set(id as i64),
        name: Set("auth_token".to_string()),
        token: Set(token_hash),
        abilities: Set(Some("[\"*\"]".to_string())),
        last_used_at: Set(None),
        expires_at: Set(Some(now + 86400 * 365)),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        ..Default::default()
    };
    pat.insert(db).await.unwrap();

    format!("Bearer {}", plain_token)
}

#[tokio::test]
async fn test_user_profile_info_and_check_login() {
    let db = setup_phase7_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 1, "alice@example.com", "password123", true).await;
    let app = app_router_with_state(state.clone());

    // 1. Unauthorized request
    let req = Request::builder()
        .uri("/api/v1/user/info")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 2. Authorized request to /info
    let req = Request::builder()
        .uri("/api/v1/user/info")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["email"], "alice@example.com");
    assert_eq!(json["data"]["balance"], 1000);
    assert_eq!(json["data"]["commission_balance"], 5000);
    assert!(json["data"]["avatar_url"]
        .as_str()
        .unwrap()
        .contains("gravatar"));

    // 3. /checkLogin
    let req = Request::builder()
        .uri("/api/v1/user/checkLogin")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["is_login"], true);
    assert_eq!(json["data"]["is_admin"], true);
}

#[tokio::test]
async fn test_user_password_change_and_security_reset() {
    let db = setup_phase7_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 2, "bob@example.com", "oldpassword123", false).await;
    let app = app_router_with_state(state.clone());

    // 1. Wrong old password
    let req = Request::builder()
        .uri("/api/v1/user/changePassword")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "old_password": "wrongpassword",
                "new_password": "newpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 2. Successful password change
    let req = Request::builder()
        .uri("/api/v1/user/changePassword")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "old_password": "oldpassword123",
                "new_password": "newpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify DB password updated
    let updated_user = User::find_by_id(2).one(&db).await.unwrap().unwrap();
    assert!(verify_password("newpassword123", &updated_user.password));

    // 3. Reset security (new uuid & token)
    let old_token = updated_user.token.clone();
    let req = Request::builder()
        .uri("/api/v1/user/resetSecurity")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let new_sub_url = json["data"].as_str().unwrap();
    assert!(!new_sub_url.contains(&old_token));

    let refreshed_user = User::find_by_id(2).one(&db).await.unwrap().unwrap();
    assert_ne!(refreshed_user.token, old_token);
    assert_ne!(refreshed_user.uuid, updated_user.uuid);

    // 4. Update remind preferences
    let req = Request::builder()
        .uri("/api/v1/user/update")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "remind_expire": false,
                "remind_traffic": false
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let final_user = User::find_by_id(2).one(&db).await.unwrap().unwrap();
    assert_eq!(final_user.remind_expire, Some(false));
    assert_eq!(final_user.remind_traffic, Some(false));
}

#[tokio::test]
async fn test_user_commission_transfer_and_stats() {
    let db = setup_phase7_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 3, "charlie@example.com", "password123", false).await;
    let app = app_router_with_state(state.clone());

    // User 3 has commission_balance = 5000, balance = 1000

    // 1. Transfer more than commission balance -> 400
    let req = Request::builder()
        .uri("/api/v1/user/transfer")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "transfer_amount": 99999
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 2. Transfer valid amount (2000 cents = $20)
    let req = Request::builder()
        .uri("/api/v1/user/transfer")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "transfer_amount": 2000
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let u = User::find_by_id(3).one(&db).await.unwrap().unwrap();
    assert_eq!(u.commission_balance, 3000);
    assert_eq!(u.balance, 3000);

    // 3. /getStat
    let req = Request::builder()
        .uri("/api/v1/user/getStat")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"].is_array());
    assert_eq!(json["data"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_user_notices_and_knowledge_with_masking() {
    let db = setup_phase7_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 4, "david@example.com", "password123", false).await;
    let app = app_router_with_state(state.clone());
    let now = chrono::Utc::now().timestamp();

    // 1. Insert notices
    let n1 = notice::ActiveModel {
        id: Set(1),
        title: Set("Maintenance Announcement".to_string()),
        content: Set("Server maintenance scheduled tonight.".to_string()),
        show: Set(true),
        sort: Set(Some(10)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    n1.insert(&db).await.unwrap();

    let req = Request::builder()
        .uri("/api/v1/user/notice/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["data"][0]["title"], "Maintenance Announcement");

    // 2. Insert Knowledge article with placeholders and access section
    let k1 = knowledge::ActiveModel {
        id: Set(1),
        language: Set("zh-CN".to_string()),
        category: Set("Getting Started".to_string()),
        title: Set("How to connect".to_string()),
        body: Set("Welcome to {{siteName}}! Your subscribe is {{subscribeUrl}} <!--access start-->Secret VIP Nodes<!--access end--> Enjoy!".to_string()),
        show: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    k1.insert(&db).await.unwrap();

    // Available user fetches knowledge -> no masking
    let req = Request::builder()
        .uri("/api/v1/user/knowledge/fetch?id=1")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let content = json["data"]["body"].as_str().unwrap();
    assert!(content.contains("Secret VIP Nodes"));
    assert!(content.contains("token-4"));

    // Banned user or expired user -> masked
    let mut u: user::ActiveModel = User::find_by_id(4).one(&db).await.unwrap().unwrap().into();
    u.expired_at = Set(Some(now - 100)); // expired
    u.update(&db).await.unwrap();

    let req = Request::builder()
        .uri("/api/v1/user/knowledge/fetch?id=1")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let content = json["data"]["body"].as_str().unwrap();
    assert!(content.contains("v2board-no-access"));
    assert!(!content.contains("Secret VIP Nodes"));

    // Fetch list grouped by category
    let req = Request::builder()
        .uri("/api/v1/user/knowledge/fetch?language=zh-CN")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["Getting Started"].is_array());
}

#[tokio::test]
async fn test_user_coupon_validation() {
    let db = setup_phase7_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 5, "eve@example.com", "password123", false).await;
    let app = app_router_with_state(state.clone());
    let now = chrono::Utc::now().timestamp();

    // 1. Insert active coupon: 20% discount (type = 2, value = 20)
    let c = coupon::ActiveModel {
        id: Set(1),
        code: Set("SAVE20".to_string()),
        name: Set("20% Off".to_string()),
        r#type: Set(2),
        value: Set(20),
        show: Set(true),
        limit_use: Set(Some(50)),
        limit_use_with_user: Set(Some(1)),
        limit_plan_ids: Set(Some("[1, 2]".to_string())),
        started_at: Set(now - 3600),
        ended_at: Set(now + 3600 * 24),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    c.insert(&db).await.unwrap();

    // Check valid coupon
    let req = Request::builder()
        .uri("/api/v1/user/coupon/check")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "code": "SAVE20",
                "plan_id": 1
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["value"], 20);

    // Plan not allowed (plan_id = 99) -> 400
    let req = Request::builder()
        .uri("/api/v1/user/coupon/check")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "code": "SAVE20",
                "plan_id": 99
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Non-existent coupon -> 400
    let req = Request::builder()
        .uri("/api/v1/user/coupon/check")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "code": "NONEXISTENT",
                "plan_id": 1
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_user_ticket_crud_and_withdrawal() {
    let db = setup_phase7_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 6, "frank@example.com", "password123", false).await;
    let app = app_router_with_state(state.clone());

    // 1. Create a ticket
    let req = Request::builder()
        .uri("/api/v1/user/ticket/save")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "subject": "Need help with VPN setup",
                "level": 2,
                "message": "Hello, how do I configure Sing-Box?"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Fetch tickets
    let req = Request::builder()
        .uri("/api/v1/user/ticket/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 1);
    let ticket_id = json["data"][0]["id"].as_i64().unwrap() as i32;

    // 3. Fetch ticket detail
    let req = Request::builder()
        .uri(format!("/api/v1/user/ticket/fetch?id={}", ticket_id))
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["message"][0]["is_me"], true);

    // 4. Reply to ticket
    let req = Request::builder()
        .uri("/api/v1/user/ticket/reply")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "id": ticket_id,
                "message": "Also, can I use Clash Meta instead?"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 5. Close ticket
    let req = Request::builder()
        .uri("/api/v1/user/ticket/close")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "id": ticket_id
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let t = Ticket::find_by_id(ticket_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(t.status, 1); // closed

    // 6. Commission withdrawal ticket
    state
        .setting_service
        .set("commission_withdraw_limit", "10")
        .await
        .unwrap();
    state
        .setting_service
        .set("commission_withdraw_method", "alipay,usdt")
        .await
        .unwrap();

    let req = Request::builder()
        .uri("/api/v1/user/ticket/withdraw")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "withdraw_method": "alipay",
                "withdraw_account": "user@alipay.com"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_user_gift_card_redeem_and_history() {
    let db = setup_phase7_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 7, "grace@example.com", "password123", false).await;
    let app = app_router_with_state(state.clone());
    let now = chrono::Utc::now().timestamp();

    // 1. Create GiftCardTemplate
    let tmpl = gift_card::template::ActiveModel {
        id: Set(1),
        name: Set("100 Yuan Balance Card".to_string()),
        description: Set(Some("Adds 100 RMB to balance".to_string())),
        r#type: Set(1),
        status: Set(1), // active
        rewards: Set(json!({
            "balance": 10000 // 100.00 RMB
        })
        .to_string()),
        theme_color: Set("#0066cc".to_string()),
        sort: Set(0),
        admin_id: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    tmpl.insert(&db).await.unwrap();

    // 2. Create GiftCardCode
    let code = gift_card::code::ActiveModel {
        id: Set(1),
        template_id: Set(1),
        code: Set("GIFT-CARD-100-TEST".to_string()),
        status: Set(0), // unused
        usage_count: Set(0),
        max_usage: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    code.insert(&db).await.unwrap();

    // 3. Types endpoint
    let req = Request::builder()
        .uri("/api/v1/user/gift-card/types")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. Check endpoint
    let req = Request::builder()
        .uri("/api/v1/user/gift-card/check")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "code": "GIFT-CARD-100-TEST"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["can_redeem"], true);

    // 5. Redeem endpoint
    let req = Request::builder()
        .uri("/api/v1/user/gift-card/redeem")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "code": "GIFT-CARD-100-TEST"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Check user balance updated: 1000 + 10000 = 11000
    let u = User::find_by_id(7).one(&db).await.unwrap().unwrap();
    assert_eq!(u.balance, 11000);

    // Code is now used -> duplicate redeem rejected
    let req = Request::builder()
        .uri("/api/v1/user/gift-card/redeem")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "code": "GIFT-CARD-100-TEST"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 6. History endpoint
    let req = Request::builder()
        .uri("/api/v1/user/gift-card/history")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["pagination"]["total"], 1);
}

#[tokio::test]
async fn test_user_servers_and_etag_caching() {
    let db = setup_phase7_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 8, "helen@example.com", "password123", false).await;
    let app = app_router_with_state(state.clone());
    let now = chrono::Utc::now().timestamp();

    // 1. Insert Server for group 1
    let s = server::ActiveModel {
        id: Set(10),
        name: Set("Tokyo 01".to_string()),
        r#type: Set("shadowsocks".to_string()),
        group_ids: Set(Some("[1]".to_string())),
        host: Set("tokyo.node.com".to_string()),
        port: Set("10086".to_string()),
        server_port: Set(10086),
        rate: Set(1.0),
        rate_time_enable: Set(false),
        show: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    s.insert(&db).await.unwrap();

    // Fetch servers -> 200 OK with ETag
    let req = Request::builder()
        .uri("/api/v1/user/server/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let etag = resp
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(!etag.is_empty());

    // Second request with If-None-Match -> 304 Not Modified
    let req = Request::builder()
        .uri("/api/v1/user/server/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::IF_NONE_MATCH, &etag)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
}
