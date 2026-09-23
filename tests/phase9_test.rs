use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, Schema, Set,
};
use serde_json::{json, Value};
use tower::ServiceExt;

use xboard_rs::{
    app_router_with_state,
    common::AppState,
    entities::{
        order, personal_access_token, plan, server, server_group, ticket, user, CommissionLog,
        Coupon, GiftCardCode, GiftCardTemplate, GiftCardUsage, InviteCode, Knowledge, Notice,
        Order, Payment, PersonalAccessToken, Plan, Server, ServerGroup, ServerMachine,
        ServerMachineLoadHistory, ServerRoute, Setting, StatUser, Ticket, TicketMessage, User,
    },
    services::{
        order_service::{STATUS_CANCELLED, STATUS_COMPLETED, STATUS_PENDING},
        AuthService, DeviceStateService, PlanService, ServerService, SettingService, UserService,
    },
    utils::{hash_password, sha256_hex},
};

async fn setup_phase9_db() -> DatabaseConnection {
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

    // Default ServerGroup
    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("Default Group".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    // Initial Plan
    let p = plan::ActiveModel {
        id: Set(1),
        group_id: Set(1),
        transfer_enable: Set(100), // 100 GB
        name: Set("Starter Plan".to_string()),
        speed_limit: Set(Some(100)),
        show: Set(true),
        sort: Set(Some(1)),
        renew: Set(true),
        sell: Set(Some(true)),
        month_price: Set(Some(2000)), // 20.00 CNY
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p.insert(&db).await.unwrap();

    // Normal User (id: 10, is_admin: false)
    let normal_user = user::ActiveModel {
        id: Set(10),
        email: Set("user@xboard.test".to_string()),
        password: Set(hash_password("user123").unwrap()),
        token: Set("user_token_12345678901234567890".to_string()),
        uuid: Set("user_uuid_10".to_string()),
        is_admin: Set(false),
        is_staff: Set(false),
        banned: Set(false),
        commission_type: Set(0),
        commission_balance: Set(0),
        balance: Set(10000), // 100.00 CNY
        t: Set(0),
        u: Set(0),
        d: Set(0),
        transfer_enable: Set(100 * 1073741824),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    normal_user.insert(&db).await.unwrap();

    // Admin User (id: 1, is_admin: true)
    let admin_user = user::ActiveModel {
        id: Set(1),
        email: Set("admin@xboard.test".to_string()),
        password: Set(hash_password("admin123").unwrap()),
        token: Set("admin_token_12345678901234567890".to_string()),
        uuid: Set("admin_uuid_1".to_string()),
        is_admin: Set(true),
        is_staff: Set(true),
        banned: Set(false),
        commission_type: Set(0),
        commission_balance: Set(0),
        balance: Set(50000),
        t: Set(0),
        u: Set(0),
        d: Set(0),
        transfer_enable: Set(100 * 1073741824),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    admin_user.insert(&db).await.unwrap();

    // PersonalAccessToken for admin
    let raw_admin_token = "admin_bearer_token_xyz_9876543210";
    let token_hash = sha256_hex(raw_admin_token.as_bytes());
    let pat = personal_access_token::ActiveModel {
        tokenable_type: Set("App\\Models\\User".to_string()),
        tokenable_id: Set(1),
        name: Set("admin_login".to_string()),
        token: Set(token_hash),
        abilities: Set(Some("[\"*\"]".to_string())),
        last_used_at: Set(None),
        expires_at: Set(Some(now + 86400 * 30)),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        ..Default::default()
    };
    pat.insert(&db).await.unwrap();

    // PersonalAccessToken for normal user
    let raw_user_token = "user_bearer_token_abc_1234567890";
    let user_token_hash = sha256_hex(raw_user_token.as_bytes());
    let user_pat = personal_access_token::ActiveModel {
        tokenable_type: Set("App\\Models\\User".to_string()),
        tokenable_id: Set(10),
        name: Set("user_login".to_string()),
        token: Set(user_token_hash),
        abilities: Set(Some("[\"*\"]".to_string())),
        last_used_at: Set(None),
        expires_at: Set(Some(now + 86400 * 30)),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        ..Default::default()
    };
    user_pat.insert(&db).await.unwrap();

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

const ADMIN_TOKEN: &str = "Bearer admin_bearer_token_xyz_9876543210";
const USER_TOKEN: &str = "Bearer user_bearer_token_abc_1234567890";

#[tokio::test]
async fn test_admin_auth_and_permission_rejection() {
    let db = setup_phase9_db().await;
    let state = create_test_state(db).await;
    let app = app_router_with_state(state);

    // 1. Request with no authorization header -> 401 Unauthorized
    let req = Request::builder()
        .uri("/api/v2/admin/plan/fetch")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 2. Request with normal user token (is_admin: false) -> 403 Forbidden
    let req = Request::builder()
        .uri("/api/v2/admin/plan/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, USER_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["message"]
        .as_str()
        .unwrap()
        .contains("Administrator privileges required"));

    // 3. Request with admin token -> 200 OK
    let req = Request::builder()
        .uri("/api/v2/admin/plan/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"].as_array().is_some());
}

#[tokio::test]
async fn test_dynamic_secure_admin_path() {
    let db = setup_phase9_db().await;
    let state = create_test_state(db.clone()).await;

    // Configure a custom secure_path
    state
        .setting_service
        .set("secure_path", "secret_admin_entry")
        .await
        .unwrap();

    let app = app_router_with_state(state);

    // 1. Access using the dynamic secure path: /api/v2/secret_admin_entry/config/fetch -> 200 OK
    let req = Request::builder()
        .uri("/api/v2/secret_admin_entry/config/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Access using an invalid path -> 404 NOT_FOUND
    let req = Request::builder()
        .uri("/api/v2/invalid_path/config/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 3. Test default fallback CRC (1627ab18) for all background endpoints called by React Admin
    let db2 = setup_phase9_db().await;
    let state2 = create_test_state(db2.clone()).await;
    let app2 = app_router_with_state(state2);

    let endpoints = [
        "/api/v2/1627ab18/stat/getStats",
        "/api/v2/1627ab18/stat/getOrder?start_date=2026-08-20&end_date=2026-09-19",
        "/api/v2/1627ab18/stat/getTrafficRank?type=node&start_time=1789747200&end_time=1789833600",
        "/api/v2/1627ab18/stat/getTrafficRank?type=user&start_time=1789747200&end_time=1789833600",
        "/api/v2/1627ab18/plugin/getPlugins",
        "/api/v2/1627ab18/plugin/types",
        "/api/v2/1627ab18/system/getQueueStats",
        "/api/v2/1627ab18/theme/getThemes",
    ];

    for ep in endpoints {
        let req = Request::builder()
            .uri(ep)
            .method("GET")
            .header(header::AUTHORIZATION, ADMIN_TOKEN)
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app2.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "Endpoint failed: {}", ep);
    }

    // Verify getStats payload structure
    let req = Request::builder()
        .uri("/api/v2/1627ab18/stat/getStats")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app2.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["todayIncome"].is_number());
    assert!(json["data"]["currentMonthIncome"].is_number());
    assert!(json["data"]["onlineNodes"].is_number());

    // Verify getOrder payload structure
    let req = Request::builder()
        .uri("/api/v2/1627ab18/stat/getOrder?start_date=2026-08-20&end_date=2026-09-19")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app2.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["summary"].is_object());
    assert!(json["data"]["list"].is_array());
}

#[tokio::test]
async fn test_admin_plan_crud_and_protections() {
    let db = setup_phase9_db().await;
    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state);

    // 1. Create a new plan via /plan/save
    let new_plan_req = json!({
        "group_id": 1,
        "transfer_enable": 200,
        "name": "Advanced Plan",
        "month_price": 5000,
        "speed_limit": 500,
        "show": true
    });
    let req = Request::builder()
        .uri("/api/v2/admin/plan/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(new_plan_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify 2 plans now exist
    let plans = Plan::find().all(&db).await.unwrap();
    assert_eq!(plans.len(), 2);
    let adv_plan = plans
        .into_iter()
        .find(|p| p.name == "Advanced Plan")
        .unwrap();

    // 1.1 Verify creating a plan with string fields as submitted by Web UI (transfer_enable: "200", month_price: "5000", show: "1", etc.)
    let string_plan_req = json!({
        "group_id": "1",
        "transfer_enable": "200",
        "name": "String Plan Test",
        "month_price": "5000",
        "speed_limit": "500",
        "show": "1",
        "renew": "true",
        "sell": "1",
        "sort": "20"
    });
    let req = Request::builder()
        .uri("/api/v2/admin/plan/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(string_plan_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let created_str_plan = Plan::find()
        .filter(plan::Column::Name.eq("String Plan Test"))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created_str_plan.transfer_enable, 200);
    assert_eq!(created_str_plan.month_price, Some(5000));
    assert!(created_str_plan.show);
    assert!(created_str_plan.renew);
    assert_eq!(created_str_plan.sell, Some(true));

    // Test plan update with string id and string bool
    let update_toggle_req = json!({
        "id": created_str_plan.id.to_string(),
        "show": "0",
        "renew": "false"
    });
    let req = Request::builder()
        .uri("/api/v2/admin/plan/update")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(update_toggle_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let updated_str_plan = Plan::find_by_id(created_str_plan.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(!updated_str_plan.show);
    assert!(!updated_str_plan.renew);

    // Test drop with string id
    let drop_str_req = json!({ "id": created_str_plan.id.to_string() });
    let req = Request::builder()
        .uri("/api/v2/admin/plan/drop")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(drop_str_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(Plan::find_by_id(created_str_plan.id)
        .one(&db)
        .await
        .unwrap()
        .is_none());

    // 2. Update plan with force_update = true
    // First, assign normal user to adv_plan.id
    let mut u_active: user::ActiveModel =
        User::find_by_id(10).one(&db).await.unwrap().unwrap().into();
    u_active.plan_id = Set(Some(adv_plan.id));
    u_active.speed_limit = Set(Some(500));
    u_active.update(&db).await.unwrap();

    let update_plan_req = json!({
        "id": adv_plan.id,
        "group_id": 1,
        "transfer_enable": 300, // increase to 300
        "name": "Advanced Plan Pro",
        "speed_limit": 1000,
        "force_update": true
    });
    let req = Request::builder()
        .uri("/api/v2/admin/plan/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(update_plan_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify user's speed_limit and transfer_enable updated due to force_update
    let updated_user = User::find_by_id(10).one(&db).await.unwrap().unwrap();
    assert_eq!(updated_user.speed_limit, Some(1000));
    assert_eq!(updated_user.transfer_enable, 300 * 1073741824);

    // 3. Drop plan prevented by user association
    let drop_req = json!({ "id": adv_plan.id });
    let req = Request::builder()
        .uri("/api/v2/admin/plan/drop")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(drop_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::OK);

    // Remove user association
    let mut u_active: user::ActiveModel = updated_user.into();
    u_active.plan_id = Set(None);
    u_active.update(&db).await.unwrap();

    // Now drop should succeed
    let req = Request::builder()
        .uri("/api/v2/admin/plan/drop")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(drop_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(Plan::find_by_id(adv_plan.id)
        .one(&db)
        .await
        .unwrap()
        .is_none());

    // 3.1 Test creating a plan using React admin frontend schema: tags as Array, prices as Object in Yuan
    let zod_plan_req = json!({
        "group_id": 1,
        "transfer_enable": 150,
        "name": "Zod Form Plan",
        "tags": ["Fast", "Game"],
        "prices": {
            "monthly": "15.50",
            "quarterly": 45.0
        },
        "speed_limit": 200,
        "show": true
    });
    let req = Request::builder()
        .uri("/api/v2/admin/plan/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(zod_plan_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify DB columns: month_price was set to 1550 cents, quarter_price to 4500 cents
    let zod_plan = Plan::find()
        .filter(plan::Column::Name.eq("Zod Form Plan"))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(zod_plan.month_price, Some(1550));
    assert_eq!(zod_plan.quarter_price, Some(4500));

    // 3.2 Test /api/v2/admin/plan/fetch returns tags as native JSON array and prices as native JSON object
    let req = Request::builder()
        .uri("/api/v2/admin/plan/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    let json_val: Value = serde_json::from_slice(&body).unwrap();
    let plans_arr = json_val["data"]
        .as_array()
        .expect("plans list must be array");
    let fetched_zod_plan = plans_arr
        .iter()
        .find(|p| p["name"] == "Zod Form Plan")
        .expect("Zod Form Plan should be in fetch result");

    // Assert tags is a native JSON Array (preventing Zod 'Expected array, received string')
    assert!(
        fetched_zod_plan["tags"].is_array(),
        "tags must be native JSON array"
    );
    assert_eq!(fetched_zod_plan["tags"][0], "Fast");
    assert_eq!(fetched_zod_plan["tags"][1], "Game");

    // Assert prices is a native JSON Object with Yuan float values
    assert!(
        fetched_zod_plan["prices"].is_object(),
        "prices must be native JSON object"
    );
    assert_eq!(fetched_zod_plan["prices"]["monthly"], 15.5);
    assert_eq!(fetched_zod_plan["prices"]["quarterly"], 45.0);

    // 4. Test plan sort
    let sort_req = json!({ "ids": [1] });
    let req = Request::builder()
        .uri("/api/v2/admin/plan/sort")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(sort_req.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_admin_server_and_node_management() {
    let db = setup_phase9_db().await;
    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state);

    // 1. Server Group: save, fetch, drop
    let grp_req = json!({ "name": "VIP Group" });
    let req = Request::builder()
        .uri("/api/v2/admin/server/group/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(grp_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let groups = ServerGroup::find().all(&db).await.unwrap();
    let vip_grp = groups.into_iter().find(|g| g.name == "VIP Group").unwrap();

    // 2. Server Route: save, fetch, drop
    let route_req = json!({
        "remarks": "Block Ads",
        "match": ["geosite:category-ads-all"],
        "action": "block"
    });
    let req = Request::builder()
        .uri("/api/v2/admin/server/route/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(route_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let routes = ServerRoute::find().all(&db).await.unwrap();
    assert_eq!(routes.len(), 1);

    // 3. Server Manage: save (create hysteria node)
    let node_req = json!({
        "type": "hysteria",
        "group_ids": [vip_grp.id],
        "name": "HK Hy2 Node 01",
        "host": "hk.xboard.test",
        "port": 443,
        "server_port": 443,
        "rate": 1.5,
        "show": 1
    });
    let req = Request::builder()
        .uri("/api/v2/admin/server/manage/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(node_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let nodes = Server::find().all(&db).await.unwrap();
    assert_eq!(nodes.len(), 1);
    let s_node = &nodes[0];
    assert_eq!(s_node.name, "HK Hy2 Node 01");
    assert_eq!(s_node.rate, 1.5);

    // 3.1 Verify server save with string-typed numeric/bool fields (as sent by web UI)
    let string_node_req = json!({
        "type": "vless",
        "name": "US Node String Rate",
        "group_ids": [vip_grp.id.to_string()],
        "host": "us.xboard.test",
        "port": "443",
        "server_port": "443",
        "rate": "1",
        "show": "1",
        "enabled": "true",
        "sort": "10"
    });
    let req = Request::builder()
        .uri("/api/v2/admin/server/manage/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(string_node_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let vless_node = Server::find()
        .filter(server::Column::Name.eq("US Node String Rate"))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(vless_node.rate, 1.0);
    assert_eq!(vless_node.server_port, 443);
    assert!(vless_node.show);
    assert_eq!(vless_node.enabled, Some(true));

    // 4. Server Manage: copy
    let copy_req = json!({ "id": s_node.id });
    let req = Request::builder()
        .uri("/api/v2/admin/server/manage/copy")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(copy_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(Server::find().count(&db).await.unwrap(), 3);

    // 5. Server Manage: getNodes
    let req = Request::builder()
        .uri("/api/v2/admin/server/manage/getNodes")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let nodes_val: Value = serde_json::from_slice(&body).unwrap();
    let nodes_arr = nodes_val["data"].as_array().unwrap();
    assert_eq!(nodes_arr.len(), 3);
    // Ensure groups is populated with { id, name } and group_ids is array of strings
    assert!(!nodes_arr[0]["groups"].as_array().unwrap().is_empty());
    assert_eq!(nodes_arr[0]["groups"][0]["name"], "VIP Group");
    assert!(nodes_arr[0]["group_ids"].as_array().unwrap()[0].is_string());

    // 5.1 Server Group: fetch - verify server_count accurately counts nodes
    let req = Request::builder()
        .uri("/api/v2/admin/server/group/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let grp_val: Value = serde_json::from_slice(&body).unwrap();
    let grp_arr = grp_val["data"].as_array().unwrap();
    let fetched_vip_grp = grp_arr.iter().find(|g| g["name"] == "VIP Group").unwrap();
    assert_eq!(fetched_vip_grp["server_count"], 3);

    // 6. Server Machine: save, resetToken, getToken, installCommand, drop
    let m_req = json!({
        "name": "Machine HK 01",
        "notes": "Hong Kong Core Gateway"
    });
    let req = Request::builder()
        .uri("/api/v2/admin/server/machine/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(m_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let save_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(save_json["data"]["token"].is_string());
    assert!(save_json["data"]["install_command"]
        .as_str()
        .unwrap()
        .contains("--mode machine"));

    let m = ServerMachine::find().one(&db).await.unwrap().unwrap();
    assert_eq!(m.name, "Machine HK 01");

    // Test server update with string machine_id and string show
    let update_node_req = json!({
        "id": vless_node.id.to_string(),
        "machine_id": m.id.to_string(),
        "show": "0"
    });
    let req = Request::builder()
        .uri("/api/v2/admin/server/manage/update")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(update_node_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bound_node = Server::find_by_id(vless_node.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bound_node.machine_id, Some(m.id));
    assert!(!bound_node.show);

    // getToken
    let req = Request::builder()
        .uri(format!("/api/v2/admin/server/machine/getToken?id={}", m.id))
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let get_token_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(get_token_json["data"]["token"].as_str().unwrap(), m.token);

    // resetToken
    let reset_req = json!({ "id": m.id });
    let req = Request::builder()
        .uri("/api/v2/admin/server/machine/resetToken")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(reset_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let reset_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(reset_json["data"]["token"].is_string());

    // installCommand
    let req = Request::builder()
        .uri(format!(
            "/api/v2/admin/server/machine/installCommand?id={}",
            m.id
        ))
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let cmd_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let cmd = cmd_json["data"]["command"].as_str().unwrap();
    assert!(cmd.contains("--mode machine"));
    assert!(cmd.contains(&format!("--machine-id {}", m.id)));

    // fetch: verify machine list returns parsed load_status object and servers_count
    let req = Request::builder()
        .uri("/api/v2/admin/server/machine/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let fetch_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let list = fetch_json["data"].as_array().unwrap();
    assert!(!list.is_empty());
    assert_eq!(list[0]["servers_count"].as_i64(), Some(1));
    assert!(list[0]["load_status"].is_null() || list[0]["load_status"].is_object());
}

#[tokio::test]
async fn test_admin_user_management_lifecycle() {
    let db = setup_phase9_db().await;
    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state);

    // 1. Paginated fetch with dynamic query
    let req = Request::builder()
        .uri("/api/v2/admin/user/fetch?current=1&pageSize=10")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"].as_i64().unwrap(), 2);

    // 2. getUserInfoById
    let req = Request::builder()
        .uri("/api/v2/admin/user/getUserInfoById?id=10")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["balance"].as_i64().unwrap(), 10000); // 10000 cents

    // 3. Update user: balance + 50 yuan, set remarks
    let update_req = json!({
        "id": 10,
        "balance": 150.0,
        "remarks": "VIP tester"
    });
    let req = Request::builder()
        .uri("/api/v2/admin/user/update")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(update_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let updated_user = User::find_by_id(10).one(&db).await.unwrap().unwrap();
    assert_eq!(updated_user.balance, 15000); // 150.00 yuan in cents
    assert_eq!(updated_user.remarks, Some("VIP tester".to_string()));

    // 4. Ban and Unban user
    let ban_req = json!({ "id": 10 });
    let req = Request::builder()
        .uri("/api/v2/admin/user/ban")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(ban_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(User::find_by_id(10).one(&db).await.unwrap().unwrap().banned);

    // Unban
    let unban_req = json!({ "id": 10, "banned": false });
    let req = Request::builder()
        .uri("/api/v2/admin/user/ban")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(unban_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(!User::find_by_id(10).one(&db).await.unwrap().unwrap().banned);

    // 5. Generate users in bulk
    let gen_req = json!({
        "generate_count": 3,
        "plan_id": 1,
        "email_prefix": "batch_user"
    });
    let req = Request::builder()
        .uri("/api/v2/admin/user/generate")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(gen_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(User::find().count(&db).await.unwrap(), 5);

    // 6. Reset Secret
    let secret_req = json!({ "id": 10 });
    let old_user = User::find_by_id(10).one(&db).await.unwrap().unwrap();
    let req = Request::builder()
        .uri("/api/v2/admin/user/resetSecret")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(secret_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let new_user = User::find_by_id(10).one(&db).await.unwrap().unwrap();
    assert_ne!(old_user.token, new_user.token);

    // 7. Dump CSV
    let dump_req = json!({});
    let req = Request::builder()
        .uri("/api/v2/admin/user/dumpCSV")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(dump_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let csv_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(csv_str.starts_with("id,email,balance,plan_id,expired_at,created_at\n"));

    // 8. Destroy user
    let del_req = json!({ "id": 10 });
    let req = Request::builder()
        .uri("/api/v2/admin/user/destroy")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(del_req.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(User::find_by_id(10).one(&db).await.unwrap().is_none());
}

#[tokio::test]
async fn test_admin_order_management_and_manual_paid() {
    let db = setup_phase9_db().await;
    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state);

    let now = chrono::Utc::now().timestamp();
    // Create a pending order for user 10
    let o = order::ActiveModel {
        user_id: Set(10),
        plan_id: Set(1),
        trade_no: Set("20260919120000000001".to_string()),
        total_amount: Set(2000),
        status: Set(STATUS_PENDING),
        period: Set("month_price".to_string()),
        r#type: Set(1),
        commission_status: Set(0),
        commission_balance: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    o.insert(&db).await.unwrap();

    // 1. Fetch orders
    let req = Request::builder()
        .uri("/api/v2/admin/order/fetch?current=1&pageSize=10")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"].as_i64().unwrap(), 1);

    // 2. Order Detail
    let detail_req = json!({ "trade_no": "20260919120000000001" });
    let req = Request::builder()
        .uri("/api/v2/admin/order/detail")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(detail_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. Mark Paid manually
    let paid_req = json!({ "trade_no": "20260919120000000001" });
    let req = Request::builder()
        .uri("/api/v2/admin/order/paid")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(paid_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify order completed and user's plan activated
    let paid_order = Order::find()
        .filter(order::Column::TradeNo.eq("20260919120000000001"))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(paid_order.status, STATUS_COMPLETED);

    let updated_user = User::find_by_id(10).one(&db).await.unwrap().unwrap();
    assert_eq!(updated_user.plan_id, Some(1));
    assert!(updated_user.expired_at.unwrap() > now);

    // 4. Create another pending order and cancel it
    let o2 = order::ActiveModel {
        user_id: Set(10),
        plan_id: Set(1),
        trade_no: Set("20260919120000000002".to_string()),
        total_amount: Set(2000),
        status: Set(STATUS_PENDING),
        period: Set("month_price".to_string()),
        r#type: Set(1),
        commission_status: Set(0),
        commission_balance: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    o2.insert(&db).await.unwrap();

    let cancel_req = json!({ "trade_no": "20260919120000000002" });
    let req = Request::builder()
        .uri("/api/v2/admin/order/cancel")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(cancel_req.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let cancelled_order = Order::find()
        .filter(order::Column::TradeNo.eq("20260919120000000002"))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled_order.status, STATUS_CANCELLED);
}

#[tokio::test]
async fn test_admin_config_management_and_cache_sync() {
    let db = setup_phase9_db().await;
    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state.clone());

    // 1. Fetch grouped config
    let req = Request::builder()
        .uri("/api/v2/admin/config/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["site"].is_object());
    assert!(json["data"]["invite"].is_object());

    // 2. Fetch specific config section via ?key=site
    let req = Request::builder()
        .uri("/api/v2/admin/config/fetch?key=site")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["site"].is_object());

    // 3. Save config and verify SettingService cache reloaded
    let save_req = json!({
        "app_name": "SuperXboard Pro",
        "app_url": "https://super.xboard.test"
    });
    let req = Request::builder()
        .uri("/api/v2/admin/config/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(save_req.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify setting_service has updated value immediately in memory
    assert_eq!(
        state.setting_service.get_string("app_name", "").await,
        "SuperXboard Pro"
    );
    assert_eq!(
        state.setting_service.get_string("app_url", "").await,
        "https://super.xboard.test"
    );
}

#[tokio::test]
async fn test_admin_stat_and_system_overview() {
    let db = setup_phase9_db().await;
    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state);

    // 1. getOverride metrics
    let req = Request::builder()
        .uri("/api/v2/admin/stat/getOverride")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"]["month_register_total"].as_i64().is_some());
    assert!(json["data"]["online_nodes"].as_i64().is_some());

    // 2. getTrafficRank
    let req = Request::builder()
        .uri("/api/v2/admin/stat/getTrafficRank")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. getSystemStatus
    let req = Request::builder()
        .uri("/api/v2/admin/system/getSystemStatus")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_admin_notice_ticket_coupon_knowledge_giftcard() {
    let db = setup_phase9_db().await;
    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state);

    // 1. Notice: save, fetch, show, sort, drop
    let notice_req = json!({
        "title": "Welcome to Xboard",
        "content": "Enjoy fast networking",
        "show": true
    });
    let req = Request::builder()
        .uri("/api/v2/admin/notice/save")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(notice_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let n = Notice::find().one(&db).await.unwrap().unwrap();
    assert_eq!(n.title, "Welcome to Xboard");

    // Toggle show
    let show_req = json!({ "id": n.id });
    let req = Request::builder()
        .uri("/api/v2/admin/notice/show")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(show_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        !Notice::find_by_id(n.id)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .show
    );

    // 2. Coupon: generate, fetch, show, drop
    let coupon_req = json!({
        "name": "Spring 20% OFF",
        "code": "SPRING20",
        "type": 2, // percentage discount
        "value": 20,
        "limit_use": 100,
        "started_at": 0,
        "ended_at": 2000000000
    });
    let req = Request::builder()
        .uri("/api/v2/admin/coupon/generate")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(coupon_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let cp = Coupon::find().one(&db).await.unwrap().unwrap();
    assert_eq!(cp.code, "SPRING20");

    // 3. Ticket: reply and close
    let now = chrono::Utc::now().timestamp();
    let t = ticket::ActiveModel {
        user_id: Set(10),
        subject: Set("Network issue".to_string()),
        level: Set(2),
        status: Set(0),
        reply_status: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let t_model = t.insert(&db).await.unwrap();

    let reply_req = json!({
        "id": t_model.id,
        "message": "We have resolved the node congestion."
    });
    let req = Request::builder()
        .uri("/api/v2/admin/ticket/reply")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(reply_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Ticket close
    let close_req = json!({ "id": t_model.id });
    let req = Request::builder()
        .uri("/api/v2/admin/ticket/close")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(close_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        Ticket::find_by_id(t_model.id)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .status,
        1
    );

    // 4. GiftCard: create template and generate codes
    let tmpl_req = json!({
        "name": "10 Yuan Balance Card",
        "type": 1,
        "rewards": { "balance": 1000 }
    });
    let req = Request::builder()
        .uri("/api/v2/admin/gift-card/create-template")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(tmpl_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let tmpl = GiftCardTemplate::find().one(&db).await.unwrap().unwrap();
    let code_req = json!({
        "template_id": tmpl.id,
        "count": 5
    });
    let req = Request::builder()
        .uri("/api/v2/admin/gift-card/generate-codes")
        .method("POST")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(code_req.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(GiftCardCode::find().count(&db).await.unwrap(), 5);

    // 5. Payment: fetch and methods
    let req = Request::builder()
        .uri("/api/v2/admin/payment/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, ADMIN_TOKEN)
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
