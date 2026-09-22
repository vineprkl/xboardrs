use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    QueryFilter, Schema, Set,
};
use serde_json::{json, Value};
use tower::ServiceExt;

use xboard_rs::{
    app_router_with_state,
    common::AppState,
    entities::{
        coupon, order, payment, personal_access_token, plan, server_group, user, CommissionLog,
        Coupon, GiftCardCode, GiftCardTemplate, GiftCardUsage, InviteCode, Knowledge, Notice,
        Order, Payment, PersonalAccessToken, Plan, Server, ServerGroup, ServerMachine,
        ServerMachineLoadHistory, ServerRoute, Setting, StatUser, Ticket, TicketMessage, User,
    },
    services::{
        order_service::{
            OrderService, STATUS_CANCELLED, STATUS_COMPLETED, STATUS_DISCOUNTED, STATUS_PENDING,
            TYPE_NEW_PURCHASE, TYPE_UPGRADE,
        },
        AuthService, CouponService, DeviceStateService, PlanService, ServerService, SettingService,
        UserService,
    },
    utils::{hash_password, sha256_hex},
};

async fn setup_phase8_db() -> DatabaseConnection {
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
    // Default server group
    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("Default Group".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    // Plan 1: Standard Monthly / Quarterly / Yearly Plan
    let p1 = plan::ActiveModel {
        id: Set(1),
        group_id: Set(1),
        transfer_enable: Set(100), // 100 GB
        name: Set("Standard Plan".to_string()),
        speed_limit: Set(Some(1000)),
        show: Set(true),
        sort: Set(Some(1)),
        renew: Set(true),
        sell: Set(Some(true)),
        month_price: Set(Some(1000)),       // 10.00 CNY
        quarter_price: Set(Some(2800)),     // 28.00 CNY
        half_year_price: Set(Some(5500)),   // 55.00 CNY
        year_price: Set(Some(10000)),       // 100.00 CNY
        two_year_price: Set(Some(19000)),   // 190.00 CNY
        three_year_price: Set(Some(27000)), // 270.00 CNY
        onetime_price: Set(Some(1500)),     // 15.00 CNY
        reset_price: Set(Some(500)),        // 5.00 CNY
        content: Set(Some("Standard Plan details".to_string())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p1.insert(&db).await.unwrap();

    // Plan 2: Pro Plan (for upgrade tests)
    let p2 = plan::ActiveModel {
        id: Set(2),
        group_id: Set(1),
        transfer_enable: Set(200), // 200 GB
        name: Set("Pro Plan".to_string()),
        speed_limit: Set(Some(2000)),
        show: Set(true),
        sort: Set(Some(2)),
        renew: Set(true),
        sell: Set(Some(true)),
        month_price: Set(Some(2000)),      // 20.00 CNY
        quarter_price: Set(Some(5500)),    // 55.00 CNY
        half_year_price: Set(Some(10000)), // 100.00 CNY
        year_price: Set(Some(19000)),      // 190.00 CNY
        content: Set(Some("Pro Plan details".to_string())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p2.insert(&db).await.unwrap();

    // Payment 1: EPay Payment Gateway
    let epay_config = json!({
        "url": "https://epay.test.com",
        "pid": "10001",
        "key": "epay_secret_key_123456",
        "type": "alipay"
    });
    let pay1 = payment::ActiveModel {
        id: Set(1),
        uuid: Set("epay-uuid-12345".to_string()),
        payment: Set("Epay".to_string()),
        name: Set("易支付".to_string()),
        icon: Set(Some("credit-card".to_string())),
        config: Set(epay_config.to_string()),
        notify_domain: Set(None),
        handling_fee_fixed: Set(Some(50)),    // 0.50 CNY fixed
        handling_fee_percent: Set(Some(1.5)), // 1.5% fee
        enable: Set(true),
        sort: Set(Some(1)),
        created_at: Set(now),
        updated_at: Set(now),
    };
    pay1.insert(&db).await.unwrap();

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
    balance: i32,
    plan_id: Option<i32>,
    expired_at: Option<i64>,
    invite_user_id: Option<i32>,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let hashed = hash_password("password123").unwrap();

    let u = user::ActiveModel {
        id: Set(id),
        invite_user_id: Set(invite_user_id),
        email: Set(email.to_string()),
        password: Set(hashed),
        balance: Set(balance),
        commission_type: Set(0),
        commission_balance: Set(0),
        t: Set(0),
        u: Set(0),
        d: Set(0),
        transfer_enable: Set(10737418240), // 10 GB
        banned: Set(false),
        is_admin: Set(false),
        is_staff: Set(false),
        uuid: Set(format!("uuid-{}", id)),
        token: Set(format!("token-{}", id)),
        group_id: Set(Some(1)),
        plan_id: Set(plan_id),
        expired_at: Set(expired_at),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    u.insert(db).await.unwrap();

    let plain_token = format!("plain_token_user_{}", id);
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
async fn test_order_save_and_duplicate_pending_prevention() {
    let db = setup_phase8_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 1, "alice@test.com", 0, None, None, None).await;
    let app = app_router_with_state(state.clone());

    // 1. Create order for Plan 1 Monthly
    let req = Request::builder()
        .uri("/api/v1/user/order/save")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "plan_id": 1,
                "period": "monthly"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let trade_no = json["data"].as_str().unwrap();
    assert!(!trade_no.is_empty());

    // 2. Attempt to create another order while pending order exists (should be blocked with 400)
    let req2 = Request::builder()
        .uri("/api/v1/user/order/save")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "plan_id": 1,
                "period": "quarterly"
            })
            .to_string(),
        ))
        .unwrap();
    let resp2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);

    // 3. Detail check
    let req3 = Request::builder()
        .uri(format!("/api/v1/user/order/detail?trade_no={}", trade_no))
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp3 = app.clone().oneshot(req3).await.unwrap();
    assert_eq!(resp3.status(), StatusCode::OK);
    let body3 = to_bytes(resp3.into_body(), usize::MAX).await.unwrap();
    let detail_json: Value = serde_json::from_slice(&body3).unwrap();
    assert_eq!(detail_json["data"]["trade_no"], trade_no);
    assert_eq!(detail_json["data"]["total_amount"], 1000); // 10.00
    assert_eq!(detail_json["data"]["period"], "month_price"); // legacy format
    assert_eq!(detail_json["data"]["status"], STATUS_PENDING);

    // 4. Status poll check
    let req4 = Request::builder()
        .uri(format!("/api/v1/user/order/check?trade_no={}", trade_no))
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp4 = app.clone().oneshot(req4).await.unwrap();
    assert_eq!(resp4.status(), StatusCode::OK);
    let body4 = to_bytes(resp4.into_body(), usize::MAX).await.unwrap();
    let check_json: Value = serde_json::from_slice(&body4).unwrap();
    assert_eq!(check_json["data"], STATUS_PENDING);
}

#[tokio::test]
async fn test_order_coupon_and_vip_discount_math() {
    let db = setup_phase8_db().await;
    let state = create_test_state(db.clone()).await;
    let now = chrono::Utc::now().timestamp();

    // Insert Coupon: 20% discount on Plan 1
    let c = coupon::ActiveModel {
        id: Set(10),
        code: Set("SAVE20".to_string()),
        name: Set("20% Off".to_string()),
        r#type: Set(1), // percentage
        value: Set(20), // 20%
        show: Set(true),
        started_at: Set(now - 3600),
        ended_at: Set(now + 3600),
        limit_use: Set(Some(100)),
        limit_use_with_user: Set(Some(1)),
        limit_plan_ids: Set(Some("[1]".to_string())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    c.insert(&db).await.unwrap();

    let bearer = insert_user_with_token(&db, 2, "bob@test.com", 0, None, None, None).await;

    // Set VIP discount 10% on user 2
    let mut u: user::ActiveModel = User::find_by_id(2).one(&db).await.unwrap().unwrap().into();
    u.discount = Set(Some(10));
    u.update(&db).await.unwrap();

    let app = app_router_with_state(state.clone());

    // Create Order with coupon
    let req = Request::builder()
        .uri("/api/v1/user/order/save")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "plan_id": 1,
                "period": "yearly",
                "coupon_code": "SAVE20"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let trade_no = json["data"].as_str().unwrap();

    // Check order detail:
    // Base yearly = 10000 (100.00 CNY)
    // 20% coupon discount = 2000 => remainder 8000
    // 10% VIP discount on 8000 = 800 => remainder 7200
    // Total discount = 2800, Total amount = 7200
    let o = Order::find()
        .filter(order::Column::TradeNo.eq(trade_no))
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(o.discount_amount, Some(2800));
    assert_eq!(o.total_amount, 7200);
    assert_eq!(o.coupon_id, Some(10));
}

#[tokio::test]
async fn test_order_free_and_balance_checkout_activation() {
    let db = setup_phase8_db().await;
    let state = create_test_state(db.clone()).await;

    // Insert user with balance = 1500 (15.00 CNY)
    let bearer = insert_user_with_token(&db, 3, "charlie@test.com", 1500, None, None, None).await;
    let app = app_router_with_state(state.clone());

    // 1. Create order for monthly plan (1000 cents)
    let req = Request::builder()
        .uri("/api/v1/user/order/save")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "plan_id": 1,
                "period": "monthly"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let trade_no = json["data"].as_str().unwrap();

    // Order total_amount should be 0 because balance (1500) covered 1000!
    // User balance should now be 500!
    let u = User::find_by_id(3).one(&db).await.unwrap().unwrap();
    assert_eq!(u.balance, 500);

    let o = Order::find()
        .filter(order::Column::TradeNo.eq(trade_no))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(o.total_amount, 0);
    assert_eq!(o.balance_amount, Some(1000));

    // 2. Checkout the 0-amount order
    let req_checkout = Request::builder()
        .uri("/api/v1/user/order/checkout")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "trade_no": trade_no
            })
            .to_string(),
        ))
        .unwrap();
    let resp_checkout = app.clone().oneshot(req_checkout).await.unwrap();
    assert_eq!(resp_checkout.status(), StatusCode::OK);
    let body_co = to_bytes(resp_checkout.into_body(), usize::MAX)
        .await
        .unwrap();
    let json_co: Value = serde_json::from_slice(&body_co).unwrap();
    assert_eq!(json_co["type"], -1);
    assert_eq!(json_co["data"], true);

    // 3. Verify order is COMPLETED and user plan is active
    let o_after = Order::find()
        .filter(order::Column::TradeNo.eq(trade_no))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(o_after.status, STATUS_COMPLETED);

    let u_after = User::find_by_id(3).one(&db).await.unwrap().unwrap();
    assert_eq!(u_after.plan_id, Some(1));
    assert_eq!(u_after.group_id, Some(1));
    assert!(u_after.expired_at.is_some());
    assert_eq!(u_after.transfer_enable, 100 * 1_073_741_824);
}

#[tokio::test]
async fn test_order_cancellation_and_balance_refund() {
    let db = setup_phase8_db().await;
    let state = create_test_state(db.clone()).await;

    // User has balance = 400 cents
    let bearer = insert_user_with_token(&db, 4, "david@test.com", 400, None, None, None).await;
    let app = app_router_with_state(state.clone());

    // 1. Create order for monthly plan (1000 cents)
    let req = Request::builder()
        .uri("/api/v1/user/order/save")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "plan_id": 1,
                "period": "monthly"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let trade_no = json["data"].as_str().unwrap();

    // User balance was partially deducted: 400 -> 0, order balance_amount = 400, total_amount = 600
    let u = User::find_by_id(4).one(&db).await.unwrap().unwrap();
    assert_eq!(u.balance, 0);

    // 2. Cancel order
    let req_cancel = Request::builder()
        .uri("/api/v1/user/order/cancel")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "trade_no": trade_no
            })
            .to_string(),
        ))
        .unwrap();
    let resp_cancel = app.clone().oneshot(req_cancel).await.unwrap();
    assert_eq!(resp_cancel.status(), StatusCode::OK);

    // 3. Verify user balance was refunded: 0 -> 400, and order is STATUS_CANCELLED
    let u_refunded = User::find_by_id(4).one(&db).await.unwrap().unwrap();
    assert_eq!(u_refunded.balance, 400);

    let o_cancelled = Order::find()
        .filter(order::Column::TradeNo.eq(trade_no))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(o_cancelled.status, STATUS_CANCELLED);
}

#[tokio::test]
async fn test_epay_gateway_checkout_and_webhook_verification() {
    let db = setup_phase8_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 5, "emma@test.com", 0, None, None, None).await;
    let app = app_router_with_state(state.clone());

    // 1. Get payment methods
    let req_pm = Request::builder()
        .uri("/api/v1/user/order/getPaymentMethod")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_pm = app.clone().oneshot(req_pm).await.unwrap();
    assert_eq!(resp_pm.status(), StatusCode::OK);
    let body_pm = to_bytes(resp_pm.into_body(), usize::MAX).await.unwrap();
    let pm_json: Value = serde_json::from_slice(&body_pm).unwrap();
    assert_eq!(pm_json["data"][0]["name"], "易支付");
    assert_eq!(pm_json["data"][0]["id"], 1);

    // 2. Create order for monthly plan (1000 cents)
    let req_save = Request::builder()
        .uri("/api/v1/user/order/save")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "plan_id": 1,
                "period": "monthly"
            })
            .to_string(),
        ))
        .unwrap();
    let resp_save = app.clone().oneshot(req_save).await.unwrap();
    assert_eq!(resp_save.status(), StatusCode::OK);
    let body_save = to_bytes(resp_save.into_body(), usize::MAX).await.unwrap();
    let json_save: Value = serde_json::from_slice(&body_save).unwrap();
    let trade_no = json_save["data"].as_str().unwrap();

    // 3. Checkout with EPay (method 1)
    // Fee: fixed 50 + 1.5% of 1000 = 15 => handling fee = 65 cents
    let req_co = Request::builder()
        .uri("/api/v1/user/order/checkout")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "trade_no": trade_no,
                "method": 1
            })
            .to_string(),
        ))
        .unwrap();
    let resp_co = app.clone().oneshot(req_co).await.unwrap();
    assert_eq!(resp_co.status(), StatusCode::OK);
    let body_co = to_bytes(resp_co.into_body(), usize::MAX).await.unwrap();
    let json_co: Value = serde_json::from_slice(&body_co).unwrap();
    assert_eq!(json_co["type"], 1); // Redirect url
    let redirect_url = json_co["data"].as_str().unwrap();
    assert!(redirect_url.contains("submit.php"));
    assert!(redirect_url.contains("sign="));
    assert!(redirect_url.contains("money=10.65")); // 10.00 + 0.65 handling fee

    // Check handling amount in DB
    let o = Order::find()
        .filter(order::Column::TradeNo.eq(trade_no))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(o.handling_amount, Some(65));
    assert_eq!(o.payment_id, Some(1));

    // 4. Test Webhook callback from EPay
    // Construct valid Epay callback signature:
    // Params: out_trade_no, trade_no, money, trade_status=TRADE_SUCCESS, type=alipay, pid=10001
    let callback_no = "EPAY_TXN_998877";
    let mut cb_params = std::collections::BTreeMap::new();
    cb_params.insert("money", "10.65");
    cb_params.insert("name", trade_no);
    cb_params.insert("out_trade_no", trade_no);
    cb_params.insert("pid", "10001");
    cb_params.insert("trade_no", callback_no);
    cb_params.insert("trade_status", "TRADE_SUCCESS");
    cb_params.insert("type", "alipay");

    let mut q_parts = Vec::new();
    for (k, v) in &cb_params {
        q_parts.push(format!("{}={}", k, v));
    }
    let q_str = q_parts.join("&");
    let sign_str = format!("{}epay_secret_key_123456", q_str);
    let valid_sign = format!("{:x}", md5::compute(sign_str.as_bytes()));

    // 4.1 Webhook with invalid sign -> 422 verify error
    let req_webhook_bad = Request::builder()
        .uri(
            "/api/v1/guest/payment/notify/Epay/epay-uuid-12345?out_trade_no=wrong&sign=invalidsign",
        )
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_webhook_bad = app.clone().oneshot(req_webhook_bad).await.unwrap();
    assert_eq!(resp_webhook_bad.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 4.2 Webhook with valid sign -> HTTP 200 "success"
    let webhook_uri = format!(
        "/api/v1/guest/payment/notify/Epay/epay-uuid-12345?{}&sign={}&sign_type=MD5",
        q_str, valid_sign
    );
    let req_webhook_good = Request::builder()
        .uri(webhook_uri)
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_webhook_good = app.clone().oneshot(req_webhook_good).await.unwrap();
    assert_eq!(resp_webhook_good.status(), StatusCode::OK);
    let body_wh = to_bytes(resp_webhook_good.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&body_wh), "success");

    // 5. Verify order is now completed and callback_no recorded
    let o_paid = Order::find()
        .filter(order::Column::TradeNo.eq(trade_no))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(o_paid.status, STATUS_COMPLETED);
    assert_eq!(o_paid.callback_no, Some(callback_no.to_string()));
    assert!(o_paid.paid_at.is_some());

    // User should have plan activated
    let u_paid = User::find_by_id(5).one(&db).await.unwrap().unwrap();
    assert_eq!(u_paid.plan_id, Some(1));
    assert!(u_paid.expired_at.is_some());
}

#[tokio::test]
async fn test_upgrade_order_surplus_value_and_discounted_orders() {
    let db = setup_phase8_db().await;
    let state = create_test_state(db.clone()).await;
    let now = chrono::Utc::now().timestamp();

    // User 6 already has Plan 1 with 30 days remaining
    let bearer = insert_user_with_token(
        &db,
        6,
        "frank@test.com",
        0,
        Some(1),
        Some(now + 86400 * 30),
        None,
    )
    .await;

    // Insert an existing completed order for Plan 1 Monthly (1000 cents = 10 CNY)
    let old_order = order::ActiveModel {
        id: Set(100),
        user_id: Set(6),
        plan_id: Set(1),
        r#type: Set(TYPE_NEW_PURCHASE),
        period: Set("monthly".to_string()),
        trade_no: Set("OLD_ORDER_100".to_string()),
        total_amount: Set(1000),
        status: Set(STATUS_COMPLETED),
        commission_status: Set(0),
        commission_balance: Set(0),
        created_at: Set(now - 86400 * 5), // 5 days ago
        updated_at: Set(now - 86400 * 5),
        paid_at: Set(Some(now - 86400 * 5)),
        ..Default::default()
    };
    old_order.insert(&db).await.unwrap();

    let app = app_router_with_state(state.clone());

    // User upgrades from Plan 1 to Plan 2 Monthly (price = 2000 cents = 20 CNY)
    let req = Request::builder()
        .uri("/api/v1/user/order/save")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "plan_id": 2,
                "period": "monthly"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let trade_no = json["data"].as_str().unwrap();

    // Check created upgrade order
    let upgrade_order = Order::find()
        .filter(order::Column::TradeNo.eq(trade_no))
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(upgrade_order.r#type, TYPE_UPGRADE);
    assert!(upgrade_order.surplus_amount.is_some());
    let surplus = upgrade_order.surplus_amount.unwrap();
    assert!(surplus > 0 && surplus <= 1000); // Between 0 and full 10.00 CNY
    assert_eq!(upgrade_order.total_amount, 2000 - surplus);
    assert!(upgrade_order
        .surplus_order_ids
        .as_ref()
        .unwrap()
        .contains("100"));

    // Activate the upgrade order
    let order_service = OrderService::new(
        db.clone(),
        state.plan_service.clone(),
        CouponService::new(db.clone()),
        state.setting_service.clone(),
    );
    order_service
        .paid(trade_no, "CALLBACK_UPGRADE_1")
        .await
        .unwrap();

    // Verify old order is now marked STATUS_DISCOUNTED (4)
    let old_order_refreshed = Order::find_by_id(100).one(&db).await.unwrap().unwrap();
    assert_eq!(old_order_refreshed.status, STATUS_DISCOUNTED);

    // Verify user now has Plan 2
    let u_upgraded = User::find_by_id(6).one(&db).await.unwrap().unwrap();
    assert_eq!(u_upgraded.plan_id, Some(2));
    assert_eq!(u_upgraded.transfer_enable, 200 * 1_073_741_824);
}

#[tokio::test]
async fn test_order_fetch_and_filter_by_status() {
    let db = setup_phase8_db().await;
    let state = create_test_state(db.clone()).await;
    let bearer = insert_user_with_token(&db, 7, "grace@test.com", 0, None, None, None).await;
    let app = app_router_with_state(state.clone());
    let now = chrono::Utc::now().timestamp();

    // Insert 2 orders for user 7: one pending, one completed
    let o1 = order::ActiveModel {
        id: Set(201),
        user_id: Set(7),
        plan_id: Set(1),
        r#type: Set(TYPE_NEW_PURCHASE),
        period: Set("monthly".to_string()),
        trade_no: Set("ORDER_P_201".to_string()),
        total_amount: Set(1000),
        status: Set(STATUS_PENDING),
        commission_status: Set(0),
        commission_balance: Set(0),
        created_at: Set(now - 100),
        updated_at: Set(now - 100),
        ..Default::default()
    };
    o1.insert(&db).await.unwrap();

    let o2 = order::ActiveModel {
        id: Set(202),
        user_id: Set(7),
        plan_id: Set(1),
        r#type: Set(TYPE_NEW_PURCHASE),
        period: Set("yearly".to_string()),
        trade_no: Set("ORDER_C_202".to_string()),
        total_amount: Set(10000),
        status: Set(STATUS_COMPLETED),
        commission_status: Set(0),
        commission_balance: Set(0),
        created_at: Set(now - 50),
        updated_at: Set(now - 50),
        paid_at: Set(Some(now - 50)),
        ..Default::default()
    };
    o2.insert(&db).await.unwrap();

    // 1. Fetch all orders
    let req = Request::builder()
        .uri("/api/v1/user/order/fetch")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 2);

    // 2. Fetch filtered by status = 3 (completed)
    let req_filtered = Request::builder()
        .uri("/api/v1/user/order/fetch?status=3")
        .method("GET")
        .header(header::AUTHORIZATION, &bearer)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_filtered = app.clone().oneshot(req_filtered).await.unwrap();
    assert_eq!(resp_filtered.status(), StatusCode::OK);
    let body_f = to_bytes(resp_filtered.into_body(), usize::MAX)
        .await
        .unwrap();
    let json_f: Value = serde_json::from_slice(&body_f).unwrap();
    assert_eq!(json_f["data"].as_array().unwrap().len(), 1);
    assert_eq!(json_f["data"][0]["trade_no"], "ORDER_C_202");
}

#[tokio::test]
async fn test_invite_commission_calculation() {
    let db = setup_phase8_db().await;
    let state = create_test_state(db.clone()).await;

    // User 8 is the inviter
    let _ = insert_user_with_token(&db, 8, "inviter@test.com", 0, None, None, None).await;

    // User 9 is invited by User 8
    let bearer9 = insert_user_with_token(&db, 9, "invitee@test.com", 0, None, None, Some(8)).await;
    let app = app_router_with_state(state.clone());

    // User 9 purchases Plan 1 Monthly (1000 cents = 10.00 CNY)
    // Default system commission rate = 10% => 100 cents (1.00 CNY)
    let req = Request::builder()
        .uri("/api/v1/user/order/save")
        .method("POST")
        .header(header::AUTHORIZATION, &bearer9)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "plan_id": 1,
                "period": "monthly"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let trade_no = json["data"].as_str().unwrap();

    let o = Order::find()
        .filter(order::Column::TradeNo.eq(trade_no))
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(o.invite_user_id, Some(8));
    assert_eq!(o.commission_balance, 100); // 10% of 1000
}
