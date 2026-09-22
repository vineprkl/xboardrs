use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    QueryFilter, Schema, Set,
};
use xboard_rs::entities::{
    coupon, order, payment, plan, server, server_group, server_machine, setting, user, Coupon,
    Order, Payment, Plan, Server, ServerGroup, ServerMachine, Setting, User,
};

/// Helper to spin up an in-memory SQLite database and create test tables
async fn setup_test_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory sqlite");

    let backend = db.get_database_backend();
    let schema = Schema::new(backend);

    let tables = vec![
        schema.create_table_from_entity(ServerGroup),
        schema.create_table_from_entity(Plan),
        schema.create_table_from_entity(User),
        schema.create_table_from_entity(ServerMachine),
        schema.create_table_from_entity(Server),
        schema.create_table_from_entity(Coupon),
        schema.create_table_from_entity(Payment),
        schema.create_table_from_entity(Order),
        schema.create_table_from_entity(Setting),
    ];

    for stmt in tables {
        db.execute(backend.build(&stmt))
            .await
            .expect("Failed to create table in test database");
    }

    db
}

#[tokio::test]
async fn test_user_and_plan_crud_and_relations() {
    let db = setup_test_db().await;
    let now = chrono::Utc::now().timestamp();

    // 1. Create Server Group
    let group = server_group::ActiveModel {
        name: Set("VIP Group".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert server group");

    assert_eq!(group.name, "VIP Group");

    // 2. Create Plan
    let plan = plan::ActiveModel {
        group_id: Set(group.id),
        transfer_enable: Set(100 * 1024 * 1024 * 1024), // 100 GB in bytes
        name: Set("Pro Plan".to_string()),
        speed_limit: Set(Some(1000)),
        show: Set(true),
        sort: Set(Some(1)),
        renew: Set(true),
        month_price: Set(Some(3000)), // 30.00 in cents
        quarter_price: Set(Some(8000)),
        year_price: Set(Some(28000)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert plan");

    assert_eq!(plan.name, "Pro Plan");
    assert_eq!(plan.month_price, Some(3000));

    // 3. Create User
    let user_model = user::ActiveModel {
        email: Set("user@example.com".to_string()),
        password: Set("$2y$10$abcdefghijklmnopqrstuv".to_string()),
        balance: Set(5000), // 50.00
        commission_type: Set(0),
        commission_balance: Set(0),
        t: Set(0),
        u: Set(1024 * 1024 * 100), // 100 MB
        d: Set(1024 * 1024 * 500), // 500 MB
        transfer_enable: Set(100 * 1024 * 1024 * 1024),
        banned: Set(false),
        is_admin: Set(false),
        is_staff: Set(false),
        uuid: Set("11111111-2222-3333-4444-555555555555".to_string()),
        token: Set("secret_token_123".to_string()),
        plan_id: Set(Some(plan.id)),
        group_id: Set(Some(group.id)),
        expired_at: Set(Some(now + 86400 * 30)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert user");

    assert_eq!(user_model.email, "user@example.com");
    assert_eq!(user_model.plan_id, Some(plan.id));

    // 4. Read User & check password serialization exclusion
    let found_user = User::find_by_id(user_model.id)
        .one(&db)
        .await
        .expect("Query failed")
        .expect("User not found");

    assert_eq!(found_user.email, "user@example.com");
    assert_eq!(found_user.u, 1024 * 1024 * 100);

    // Verify password is NOT leaked in JSON serialization
    let user_json = serde_json::to_value(&found_user).unwrap();
    assert!(user_json.get("password").is_none());
    assert_eq!(user_json["email"], "user@example.com");

    // 5. Update user traffic and balance
    let mut user_active: user::ActiveModel = found_user.into();
    user_active.u = Set(1024 * 1024 * 200); // uploaded 200 MB
    user_active.balance = Set(2000); // deducted 3000
    let updated_user = user_active.update(&db).await.expect("Update failed");

    assert_eq!(updated_user.u, 1024 * 1024 * 200);
    assert_eq!(updated_user.balance, 2000);
}

#[tokio::test]
async fn test_server_and_machine_crud() {
    let db = setup_test_db().await;
    let now = chrono::Utc::now().timestamp();

    // 1. Create Machine
    let machine = server_machine::ActiveModel {
        name: Set("HK-Node-01".to_string()),
        token: Set("machine_token_xyz".to_string()),
        is_active: Set(true),
        last_seen_at: Set(Some(now)),
        load_status: Set(Some(r#"{"cpu":15.5,"mem":42.0}"#.to_string())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert machine");

    // 2. Create Server (VLESS Reality node)
    let srv = server::ActiveModel {
        name: Set("HK VLESS 01".to_string()),
        r#type: Set("vless".to_string()),
        code: Set(Some("hk-vless-01".to_string())),
        machine_id: Set(Some(machine.id)),
        host: Set("hk01.example.com".to_string()),
        port: Set("443".to_string()),
        server_port: Set(443),
        rate: Set(1.5),
        rate_time_enable: Set(false),
        group_ids: Set(Some("[1]".to_string())),
        route_ids: Set(Some("[]".to_string())),
        tags: Set(Some(r#"["HK","Fast"]"#.to_string())),
        protocol_settings: Set(Some(r#"{"flow":"xtls-rprx-vision","tls":1}"#.to_string())),
        show: Set(true),
        enabled: Set(Some(true)),
        sort: Set(Some(10)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert server");

    assert_eq!(srv.name, "HK VLESS 01");
    assert_eq!(srv.rate, 1.5);
    assert_eq!(srv.machine_id, Some(machine.id));

    // Query server by type
    let servers = Server::find()
        .filter(server::Column::Type.eq("vless"))
        .all(&db)
        .await
        .expect("Query failed");

    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].host, "hk01.example.com");
}

#[tokio::test]
async fn test_order_and_coupon_lifecycle() {
    let db = setup_test_db().await;
    let now = chrono::Utc::now().timestamp();

    // 1. Create Coupon (15% off)
    let coupon_item = coupon::ActiveModel {
        code: Set("DISCOUNT15".to_string()),
        name: Set("15% OFF Promo".to_string()),
        r#type: Set(1), // percentage
        value: Set(15), // 15%
        show: Set(true),
        limit_use: Set(Some(100)),
        started_at: Set(now - 3600),
        ended_at: Set(now + 86400 * 7),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert coupon");

    // 2. Create prerequisite Group & Plan & User & Payment
    let group = server_group::ActiveModel {
        name: Set("Default Group".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert group");

    let plan = plan::ActiveModel {
        group_id: Set(group.id),
        transfer_enable: Set(1000),
        name: Set("Starter Plan".to_string()),
        show: Set(true),
        renew: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert plan");

    let usr = user::ActiveModel {
        email: Set("buyer@example.com".to_string()),
        password: Set("hashed_pw".to_string()),
        balance: Set(0),
        commission_type: Set(0),
        commission_balance: Set(0),
        t: Set(0),
        u: Set(0),
        d: Set(0),
        transfer_enable: Set(1000),
        banned: Set(false),
        is_admin: Set(false),
        is_staff: Set(false),
        uuid: Set("buyer-uuid-123".to_string()),
        token: Set("buyer-token-123".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert user");

    let pmt = payment::ActiveModel {
        uuid: Set("alipay-uuid-1".to_string()),
        payment: Set("AlipayF2f".to_string()),
        name: Set("支付宝当面付".to_string()),
        config: Set("{}".to_string()),
        enable: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert payment");

    // 3. Create Order
    let order_item = order::ActiveModel {
        user_id: Set(usr.id),
        plan_id: Set(plan.id),
        coupon_id: Set(Some(coupon_item.id)),
        payment_id: Set(Some(pmt.id)),
        r#type: Set(1), // new purchase
        period: Set("monthly".to_string()),
        trade_no: Set("202609191234567890".to_string()),
        total_amount: Set(2550), // 3000 * 0.85 = 2550 cents
        discount_amount: Set(Some(450)),
        status: Set(0), // STATUS_PENDING
        commission_status: Set(0),
        commission_balance: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Failed to insert order");

    assert_eq!(order_item.total_amount, 2550);
    assert_eq!(order_item.status, 0);

    // 3. Complete order
    let mut order_active: order::ActiveModel = order_item.into();
    order_active.status = Set(3); // STATUS_COMPLETED
    order_active.paid_at = Set(Some(now));
    let completed_order = order_active
        .update(&db)
        .await
        .expect("Failed to update order");

    assert_eq!(completed_order.status, 3);
    assert!(completed_order.paid_at.is_some());
}

#[tokio::test]
async fn test_setting_key_value_store() {
    let db = setup_test_db().await;
    let now = chrono::Utc::now().timestamp();

    // 1. Insert settings
    let s1 = setting::ActiveModel {
        name: Set("app_name".to_string()),
        value: Set(Some("Xboard Pro".to_string())),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("Insert setting failed");

    assert_eq!(s1.name, "app_name");

    // 2. Query setting by name
    let found = Setting::find()
        .filter(setting::Column::Name.eq("app_name"))
        .one(&db)
        .await
        .expect("Query failed")
        .expect("Setting not found");

    assert_eq!(found.value.as_deref(), Some("Xboard Pro"));
}
