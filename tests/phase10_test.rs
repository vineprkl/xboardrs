use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, Schema, Set,
};

use xboard_rs::{
    common::AppState,
    cron::{
        calculate_next_reset_time, check_and_cleanup_nodes, check_and_close_tickets,
        check_and_pay_commissions, check_and_reset_traffic, check_orders, clean_logs,
        record_daily_stats,
        traffic_resetter::{
            RESET_TRAFFIC_FIRST_DAY_MONTH, RESET_TRAFFIC_FIRST_DAY_YEAR,
            RESET_TRAFFIC_FOLLOW_SYSTEM, RESET_TRAFFIC_MONTHLY, RESET_TRAFFIC_NEVER,
            RESET_TRAFFIC_YEARLY,
        },
    },
    entities::{
        admin_audit_log, commission_log, order, plan, server_group,
        stat::{server as stat_server, user as stat_user},
        ticket, traffic_reset_log, user, AdminAuditLog, CommissionLog, Coupon, GiftCardCode,
        GiftCardTemplate, GiftCardUsage, InviteCode, Knowledge, Notice, Order, Payment,
        PersonalAccessToken, Plan, Server, ServerGroup, ServerMachine, ServerMachineLoadHistory,
        ServerRoute, Setting, Stat, StatServer, StatUser, Ticket, TicketMessage, TrafficResetLog,
        User,
    },
    services::{
        order_service::{STATUS_CANCELLED, STATUS_COMPLETED, STATUS_PENDING},
        AuthService, DeviceStateService, PlanService, ServerService, SettingService, UserService,
    },
};

fn create_test_user(id: i32, email: &str) -> user::ActiveModel {
    let now = Utc::now().timestamp();
    user::ActiveModel {
        id: Set(id),
        email: Set(email.to_string()),
        password: Set("dummy_pass".to_string()),
        balance: Set(0),
        commission_type: Set(0),
        commission_balance: Set(0),
        t: Set(0),
        u: Set(0),
        d: Set(0),
        transfer_enable: Set(100 * 1073741824),
        banned: Set(false),
        is_admin: Set(false),
        is_staff: Set(false),
        uuid: Set(format!("uuid_{}", id)),
        token: Set(format!("token_{}", id)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

fn create_test_plan(id: i32, name: &str) -> plan::ActiveModel {
    let now = Utc::now().timestamp();
    plan::ActiveModel {
        id: Set(id),
        group_id: Set(1),
        transfer_enable: Set(100),
        name: Set(name.to_string()),
        show: Set(true),
        renew: Set(true),
        month_price: Set(Some(2000)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

async fn setup_phase10_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory sqlite");

    // Disable foreign keys for unit test isolation
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

    let now = Utc::now().timestamp();

    // Default ServerGroup
    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("Default Group".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    // Default Plan
    let p = create_test_plan(1, "Starter Plan");
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

#[test]
fn test_calculate_next_reset_time_logic() {
    // 1. NEVER
    assert_eq!(
        calculate_next_reset_time(Some(1700000000), Some(RESET_TRAFFIC_NEVER), 1, 1700000000),
        None
    );

    // 2. FIRST_DAY_MONTH
    // 2026-03-15 10:00:00 UTC -> 2026-04-01 00:00:00 UTC
    let now_dt = NaiveDate::from_ymd_opt(2026, 3, 15)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let now_ts = Utc.from_utc_datetime(&now_dt).timestamp();
    let next_ts = calculate_next_reset_time(
        None,
        Some(RESET_TRAFFIC_FIRST_DAY_MONTH),
        RESET_TRAFFIC_MONTHLY,
        now_ts,
    );
    let expected_dt = NaiveDate::from_ymd_opt(2026, 4, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    assert_eq!(
        next_ts,
        Some(Utc.from_utc_datetime(&expected_dt).timestamp())
    );

    // December transition to January of next year
    let dec_dt = NaiveDate::from_ymd_opt(2026, 12, 10)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let dec_ts = Utc.from_utc_datetime(&dec_dt).timestamp();
    let next_dec = calculate_next_reset_time(
        None,
        Some(RESET_TRAFFIC_FIRST_DAY_MONTH),
        RESET_TRAFFIC_MONTHLY,
        dec_ts,
    );
    let exp_jan = NaiveDate::from_ymd_opt(2027, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    assert_eq!(next_dec, Some(Utc.from_utc_datetime(&exp_jan).timestamp()));

    // 3. FIRST_DAY_YEAR
    let next_yr = calculate_next_reset_time(
        None,
        Some(RESET_TRAFFIC_FIRST_DAY_YEAR),
        RESET_TRAFFIC_MONTHLY,
        now_ts,
    );
    assert_eq!(next_yr, Some(Utc.from_utc_datetime(&exp_jan).timestamp()));

    // 4. MONTHLY on expire day
    // Expire is on the 20th of the month at 15:30:00
    let exp_dt = NaiveDate::from_ymd_opt(2026, 1, 20)
        .unwrap()
        .and_hms_opt(15, 30, 0)
        .unwrap();
    let exp_ts = Utc.from_utc_datetime(&exp_dt).timestamp();

    // Now is 2026-03-10 (before the 20th), so next reset should be 2026-03-20 15:30:00
    let next_monthly_curr = calculate_next_reset_time(
        Some(exp_ts),
        Some(RESET_TRAFFIC_MONTHLY),
        RESET_TRAFFIC_MONTHLY,
        now_ts,
    );
    let exp_monthly_curr = NaiveDate::from_ymd_opt(2026, 3, 20)
        .unwrap()
        .and_hms_opt(15, 30, 0)
        .unwrap();
    assert_eq!(
        next_monthly_curr,
        Some(Utc.from_utc_datetime(&exp_monthly_curr).timestamp())
    );

    // Now is 2026-03-25 (after the 20th), so next reset should be 2026-04-20 15:30:00
    let now_dt_late = NaiveDate::from_ymd_opt(2026, 3, 25)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let now_ts_late = Utc.from_utc_datetime(&now_dt_late).timestamp();
    let next_monthly_next = calculate_next_reset_time(
        Some(exp_ts),
        Some(RESET_TRAFFIC_MONTHLY),
        RESET_TRAFFIC_MONTHLY,
        now_ts_late,
    );
    let exp_monthly_next = NaiveDate::from_ymd_opt(2026, 4, 20)
        .unwrap()
        .and_hms_opt(15, 30, 0)
        .unwrap();
    assert_eq!(
        next_monthly_next,
        Some(Utc.from_utc_datetime(&exp_monthly_next).timestamp())
    );

    // End-of-month handling: Expire on 31st
    let exp_dt_31 = NaiveDate::from_ymd_opt(2025, 1, 31)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let exp_ts_31 = Utc.from_utc_datetime(&exp_dt_31).timestamp();

    // In February 2025 (non-leap, 28 days), target is 28th
    let feb_2025 = NaiveDate::from_ymd_opt(2025, 2, 5)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let feb_ts_2025 = Utc.from_utc_datetime(&feb_2025).timestamp();
    let next_feb_28 = calculate_next_reset_time(
        Some(exp_ts_31),
        Some(RESET_TRAFFIC_MONTHLY),
        RESET_TRAFFIC_MONTHLY,
        feb_ts_2025,
    );
    let exp_feb_28 = NaiveDate::from_ymd_opt(2025, 2, 28)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    assert_eq!(
        next_feb_28,
        Some(Utc.from_utc_datetime(&exp_feb_28).timestamp())
    );

    // In February 2024 (leap year, 29 days), target is 29th
    let feb_2024 = NaiveDate::from_ymd_opt(2024, 2, 5)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let feb_ts_2024 = Utc.from_utc_datetime(&feb_2024).timestamp();
    let next_feb_29 = calculate_next_reset_time(
        Some(exp_ts_31),
        Some(RESET_TRAFFIC_MONTHLY),
        RESET_TRAFFIC_MONTHLY,
        feb_ts_2024,
    );
    let exp_feb_29 = NaiveDate::from_ymd_opt(2024, 2, 29)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    assert_eq!(
        next_feb_29,
        Some(Utc.from_utc_datetime(&exp_feb_29).timestamp())
    );

    // 5. YEARLY
    let exp_yearly = NaiveDate::from_ymd_opt(2025, 8, 15)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let exp_yearly_ts = Utc.from_utc_datetime(&exp_yearly).timestamp();
    // Now is 2026-03-15: next reset should be 2026-08-15
    let next_yr_1 = calculate_next_reset_time(
        Some(exp_yearly_ts),
        Some(RESET_TRAFFIC_YEARLY),
        RESET_TRAFFIC_MONTHLY,
        now_ts,
    );
    let exp_yr_1 = NaiveDate::from_ymd_opt(2026, 8, 15)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    assert_eq!(
        next_yr_1,
        Some(Utc.from_utc_datetime(&exp_yr_1).timestamp())
    );

    // 6. System fallback when plan method is None or -1
    let fallback_none =
        calculate_next_reset_time(None, None, RESET_TRAFFIC_FIRST_DAY_MONTH, now_ts);
    assert_eq!(
        fallback_none,
        Some(Utc.from_utc_datetime(&expected_dt).timestamp())
    );

    let fallback_neg = calculate_next_reset_time(
        None,
        Some(RESET_TRAFFIC_FOLLOW_SYSTEM),
        RESET_TRAFFIC_FIRST_DAY_MONTH,
        now_ts,
    );
    assert_eq!(
        fallback_neg,
        Some(Utc.from_utc_datetime(&expected_dt).timestamp())
    );
}

#[tokio::test]
async fn test_traffic_reset_worker() {
    let db = setup_phase10_db().await;
    let state = create_test_state(db.clone()).await;
    let now = Utc::now().timestamp();

    // 1. Insert Plan with Monthly reset
    let mut p = create_test_plan(10, "Monthly Reset Plan");
    p.reset_traffic_method = Set(Some(RESET_TRAFFIC_MONTHLY));
    p.insert(&db).await.unwrap();

    // 2. User 1: Active, due for reset (next_reset_at <= now), has traffic
    let mut u1 = create_test_user(1, "user1@reset.com");
    u1.plan_id = Set(Some(10));
    u1.u = Set(1024 * 1024 * 100); // 100 MB
    u1.d = Set(1024 * 1024 * 500); // 500 MB
    u1.next_reset_at = Set(Some(now - 100));
    u1.reset_count = Set(Some(0));
    u1.expired_at = Set(Some(now + 30 * 86400));
    u1.insert(&db).await.unwrap();

    // User 2: Expired (expired_at <= now) -> Should NOT reset
    let mut u2 = create_test_user(2, "user2@expired.com");
    u2.plan_id = Set(Some(10));
    u2.u = Set(5000);
    u2.d = Set(5000);
    u2.next_reset_at = Set(Some(now - 100));
    u2.expired_at = Set(Some(now - 10));
    u2.insert(&db).await.unwrap();

    // User 3: Banned -> Should NOT reset
    let mut u3 = create_test_user(3, "user3@banned.com");
    u3.plan_id = Set(Some(10));
    u3.u = Set(5000);
    u3.d = Set(5000);
    u3.next_reset_at = Set(Some(now - 100));
    u3.expired_at = Set(Some(now + 30 * 86400));
    u3.banned = Set(true);
    u3.insert(&db).await.unwrap();

    // Execute reset worker
    let reset_count = check_and_reset_traffic(&state).await.unwrap();
    assert_eq!(reset_count, 1);

    // Verify User 1 was reset
    let user1 = User::find_by_id(1).one(&db).await.unwrap().unwrap();
    assert_eq!(user1.u, 0);
    assert_eq!(user1.d, 0);
    assert_eq!(user1.reset_count, Some(1));
    assert!(user1.last_reset_at.is_some());
    assert!(user1.next_reset_at.unwrap() > now);

    // Verify User 2 and User 3 were untouched
    let user2 = User::find_by_id(2).one(&db).await.unwrap().unwrap();
    assert_eq!(user2.u, 5000);
    let user3 = User::find_by_id(3).one(&db).await.unwrap().unwrap();
    assert_eq!(user3.u, 5000);

    // Verify TrafficResetLog was created for User 1
    let logs = TrafficResetLog::find()
        .filter(traffic_reset_log::Column::UserId.eq(1))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].old_upload, 1024 * 1024 * 100);
    assert_eq!(logs[0].old_download, 1024 * 1024 * 500);
    assert_eq!(logs[0].old_total, 1024 * 1024 * 600);
    assert_eq!(logs[0].new_upload, 0);
    assert_eq!(logs[0].new_download, 0);
    assert_eq!(logs[0].trigger_source, "auto");

    // Re-running immediately should reset 0 users
    let second_run = check_and_reset_traffic(&state).await.unwrap();
    assert_eq!(second_run, 0);
}

#[tokio::test]
async fn test_order_checker_worker() {
    let db = setup_phase10_db().await;
    let state = create_test_state(db.clone()).await;
    let now = Utc::now().timestamp();

    // Set order timeout to 60 minutes
    state
        .setting_service
        .set("order_timeout", "60")
        .await
        .unwrap();

    let u = create_test_user(100, "orderuser@test.com");
    u.insert(&db).await.unwrap();

    // Order 1: Pending, created 120 minutes ago (expired)
    let o1 = order::ActiveModel {
        id: Set(1),
        user_id: Set(100),
        plan_id: Set(1),
        r#type: Set(1),
        period: Set("month_price".to_string()),
        trade_no: Set("ORDER_EXPIRED_101".to_string()),
        total_amount: Set(3000),
        status: Set(STATUS_PENDING),
        commission_status: Set(0),
        commission_balance: Set(0),
        created_at: Set(now - 7200),
        updated_at: Set(now - 7200),
        ..Default::default()
    };
    o1.insert(&db).await.unwrap();

    // Order 2: Pending, created 10 minutes ago (active)
    let o2 = order::ActiveModel {
        id: Set(2),
        user_id: Set(100),
        plan_id: Set(1),
        r#type: Set(1),
        period: Set("month_price".to_string()),
        trade_no: Set("ORDER_ACTIVE_102".to_string()),
        total_amount: Set(3000),
        status: Set(STATUS_PENDING),
        commission_status: Set(0),
        commission_balance: Set(0),
        created_at: Set(now - 600),
        updated_at: Set(now - 600),
        ..Default::default()
    };
    o2.insert(&db).await.unwrap();

    // Run check_orders
    let cancelled = check_orders(&state).await.unwrap();
    assert_eq!(cancelled, 1);

    // Verify Order 1 is cancelled
    let o1_db = Order::find_by_id(1).one(&db).await.unwrap().unwrap();
    assert_eq!(o1_db.status, STATUS_CANCELLED);

    // Verify Order 2 is still pending
    let o2_db = Order::find_by_id(2).one(&db).await.unwrap().unwrap();
    assert_eq!(o2_db.status, STATUS_PENDING);
}

#[tokio::test]
async fn test_commission_checker_worker() {
    let db = setup_phase10_db().await;
    let state = create_test_state(db.clone()).await;
    let now = Utc::now().timestamp();

    // --- Scenario A: Auto-check 3-day rule + single inviter commission ---
    let inviter1 = create_test_user(10, "inviter1@test.com");
    inviter1.insert(&db).await.unwrap();

    let mut buyer1 = create_test_user(11, "buyer1@test.com");
    buyer1.invite_user_id = Set(Some(10));
    buyer1.insert(&db).await.unwrap();

    // Order completed 4 days ago with commission_status = 0 (PENDING)
    let order1 = order::ActiveModel {
        id: Set(10),
        user_id: Set(11),
        plan_id: Set(1),
        r#type: Set(1),
        period: Set("month_price".to_string()),
        trade_no: Set("TRADE_COMM_10".to_string()),
        total_amount: Set(10000),
        status: Set(STATUS_COMPLETED),
        commission_status: Set(0),
        commission_balance: Set(2000), // 20.00 commission
        invite_user_id: Set(Some(10)),
        created_at: Set(now - 4 * 86400),
        updated_at: Set(now - 4 * 86400),
        ..Default::default()
    };
    order1.insert(&db).await.unwrap();

    let paid_count = check_and_pay_commissions(&state).await.unwrap();
    assert_eq!(paid_count, 1);

    // Verify order1 commission_status is now 2 (PAID)
    let o1_after = Order::find_by_id(10).one(&db).await.unwrap().unwrap();
    assert_eq!(o1_after.commission_status, 2);
    assert_eq!(o1_after.actual_commission_balance, Some(2000));

    // Verify Inviter 1 got the commission
    let inv1_after = User::find_by_id(10).one(&db).await.unwrap().unwrap();
    assert_eq!(inv1_after.commission_balance, 2000);

    // Verify CommissionLog was inserted
    let logs = CommissionLog::find()
        .filter(commission_log::Column::TradeNo.eq("TRADE_COMM_10"))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].invite_user_id, 10);
    assert_eq!(logs[0].get_amount, 2000);

    // --- Scenario B: Multi-tier 3-level distribution ---
    state
        .setting_service
        .set("commission_distribution_enable", "1")
        .await
        .unwrap();
    state
        .setting_service
        .set("commission_distribution_l1", "50")
        .await
        .unwrap();
    state
        .setting_service
        .set("commission_distribution_l2", "30")
        .await
        .unwrap();
    state
        .setting_service
        .set("commission_distribution_l3", "20")
        .await
        .unwrap();

    // User 20 -> invited User 21 -> invited User 22 -> invited User 23 (Buyer)
    let u20 = create_test_user(20, "l3@test.com");
    u20.insert(&db).await.unwrap();

    let mut u21 = create_test_user(21, "l2@test.com");
    u21.invite_user_id = Set(Some(20));
    u21.insert(&db).await.unwrap();

    let mut u22 = create_test_user(22, "l1@test.com");
    u22.invite_user_id = Set(Some(21));
    u22.insert(&db).await.unwrap();

    let mut u23 = create_test_user(23, "buyer2@test.com");
    u23.invite_user_id = Set(Some(22));
    u23.insert(&db).await.unwrap();

    // Order already in commission_status = 1 (VALID), commission_balance = 1000
    let order2 = order::ActiveModel {
        id: Set(20),
        user_id: Set(23),
        plan_id: Set(1),
        r#type: Set(1),
        period: Set("month_price".to_string()),
        trade_no: Set("TRADE_MULTITIER_20".to_string()),
        total_amount: Set(10000),
        status: Set(STATUS_COMPLETED),
        commission_status: Set(1), // VALID
        commission_balance: Set(1000),
        invite_user_id: Set(Some(22)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    order2.insert(&db).await.unwrap();

    let paid_multi = check_and_pay_commissions(&state).await.unwrap();
    assert_eq!(paid_multi, 1);

    // Verify L1 (User 22) received 50% (500)
    let l1_user = User::find_by_id(22).one(&db).await.unwrap().unwrap();
    assert_eq!(l1_user.commission_balance, 500);

    // Verify L2 (User 21) received 30% (300)
    let l2_user = User::find_by_id(21).one(&db).await.unwrap().unwrap();
    assert_eq!(l2_user.commission_balance, 300);

    // Verify L3 (User 20) received 20% (200)
    let l3_user = User::find_by_id(20).one(&db).await.unwrap().unwrap();
    assert_eq!(l3_user.commission_balance, 200);

    // Total actual commission balance recorded on order
    let o2_after = Order::find_by_id(20).one(&db).await.unwrap().unwrap();
    assert_eq!(o2_after.actual_commission_balance, Some(1000));
}

#[tokio::test]
async fn test_ticket_checker_worker() {
    let db = setup_phase10_db().await;
    let state = create_test_state(db.clone()).await;
    let now = Utc::now().timestamp();

    let u = create_test_user(1, "ticketuser@test.com");
    u.insert(&db).await.unwrap();

    // Ticket 1: Open, replied, updated 25 hours ago -> Should close
    let t1 = ticket::ActiveModel {
        id: Set(1),
        user_id: Set(1),
        subject: Set("Inactive replied ticket".to_string()),
        level: Set(1),
        status: Set(0),       // OPEN
        reply_status: Set(1), // REPLIED
        created_at: Set(now - 30 * 3600),
        updated_at: Set(now - 25 * 3600),
    };
    t1.insert(&db).await.unwrap();

    // Ticket 2: Open, replied, updated 5 hours ago -> Should NOT close
    let t2 = ticket::ActiveModel {
        id: Set(2),
        user_id: Set(1),
        subject: Set("Recent replied ticket".to_string()),
        level: Set(1),
        status: Set(0),       // OPEN
        reply_status: Set(1), // REPLIED
        created_at: Set(now - 10 * 3600),
        updated_at: Set(now - 5 * 3600),
    };
    t2.insert(&db).await.unwrap();

    // Ticket 3: Open, unreplied, updated 40 hours ago -> Should NOT close
    let t3 = ticket::ActiveModel {
        id: Set(3),
        user_id: Set(1),
        subject: Set("Unreplied ticket".to_string()),
        level: Set(1),
        status: Set(0),       // OPEN
        reply_status: Set(0), // UNREPLIED
        created_at: Set(now - 40 * 3600),
        updated_at: Set(now - 40 * 3600),
    };
    t3.insert(&db).await.unwrap();

    let closed = check_and_close_tickets(&state).await.unwrap();
    assert_eq!(closed, 1);

    let t1_db = Ticket::find_by_id(1).one(&db).await.unwrap().unwrap();
    assert_eq!(t1_db.status, 1); // CLOSED

    let t2_db = Ticket::find_by_id(2).one(&db).await.unwrap().unwrap();
    assert_eq!(t2_db.status, 0); // OPEN

    let t3_db = Ticket::find_by_id(3).one(&db).await.unwrap().unwrap();
    assert_eq!(t3_db.status, 0); // OPEN
}

#[tokio::test]
async fn test_log_cleaner_worker() {
    let db = setup_phase10_db().await;
    let state = create_test_state(db.clone()).await;
    let now = Utc::now().timestamp();

    let u = create_test_user(1, "loguser@test.com");
    u.insert(&db).await.unwrap();

    // 1. Old StatUser (70 days ago) and Recent StatUser (10 days ago)
    let stat_u_old = stat_user::ActiveModel {
        id: Set(1),
        user_id: Set(1),
        server_rate: Set(1.0),
        u: Set(100),
        d: Set(200),
        record_type: Set("d".to_string()),
        record_at: Set(now - 70 * 86400),
        created_at: Set(now - 70 * 86400),
        updated_at: Set(now - 70 * 86400),
    };
    stat_u_old.insert(&db).await.unwrap();

    let stat_u_recent = stat_user::ActiveModel {
        id: Set(2),
        user_id: Set(1),
        server_rate: Set(1.0),
        u: Set(100),
        d: Set(200),
        record_type: Set("d".to_string()),
        record_at: Set(now - 10 * 86400),
        created_at: Set(now - 10 * 86400),
        updated_at: Set(now - 10 * 86400),
    };
    stat_u_recent.insert(&db).await.unwrap();

    // 2. Old StatServer (70 days ago)
    let stat_s_old = stat_server::ActiveModel {
        id: Set(1),
        server_id: Set(1),
        server_type: Set("v2ray".to_string()),
        u: Set(500),
        d: Set(600),
        record_type: Set("d".to_string()),
        record_at: Set(now - 70 * 86400),
        created_at: Set(now - 70 * 86400),
        updated_at: Set(now - 70 * 86400),
    };
    stat_s_old.insert(&db).await.unwrap();

    // 3. Old AdminAuditLog (100 days ago) and Recent (10 days ago)
    let audit_old = admin_audit_log::ActiveModel {
        id: Set(1),
        admin_id: Set(1),
        action: Set("login".to_string()),
        method: Set("POST".to_string()),
        uri: Set("/api/v1/admin/login".to_string()),
        created_at: Set(now - 100 * 86400),
        updated_at: Set(now - 100 * 86400),
        ..Default::default()
    };
    audit_old.insert(&db).await.unwrap();

    let audit_recent = admin_audit_log::ActiveModel {
        id: Set(2),
        admin_id: Set(1),
        action: Set("view".to_string()),
        method: Set("GET".to_string()),
        uri: Set("/api/v1/admin/user".to_string()),
        created_at: Set(now - 10 * 86400),
        updated_at: Set(now - 10 * 86400),
        ..Default::default()
    };
    audit_recent.insert(&db).await.unwrap();

    let (del_user_stat, del_server_stat, del_audits) = clean_logs(&state).await.unwrap();
    assert_eq!(del_user_stat, 1);
    assert_eq!(del_server_stat, 1);
    assert_eq!(del_audits, 1);

    // Verify recent records remain
    assert_eq!(StatUser::find().count(&db).await.unwrap(), 1);
    assert_eq!(StatServer::find().count(&db).await.unwrap(), 0);
    assert_eq!(AdminAuditLog::find().count(&db).await.unwrap(), 1);
}

#[tokio::test]
async fn test_stat_recorder_worker() {
    let db = setup_phase10_db().await;
    let state = create_test_state(db.clone()).await;

    let now = Utc::now();
    let today_date = NaiveDate::from_ymd_opt(now.year(), now.month(), now.day()).unwrap();
    let end_at = Utc
        .from_utc_datetime(&today_date.and_hms_opt(0, 0, 0).unwrap())
        .timestamp();
    let yesterday_midday = end_at - 43200; // 12 hours before today 00:00:00

    let u_inviter = create_test_user(2, "inviter@test.com");
    u_inviter.insert(&db).await.unwrap();

    // Completed Order yesterday
    let o = order::ActiveModel {
        id: Set(1),
        user_id: Set(1),
        plan_id: Set(1),
        r#type: Set(1),
        period: Set("month_price".to_string()),
        trade_no: Set("TRADE_YESTERDAY".to_string()),
        total_amount: Set(5000),
        status: Set(STATUS_COMPLETED),
        commission_status: Set(0),
        commission_balance: Set(0),
        created_at: Set(yesterday_midday),
        updated_at: Set(yesterday_midday),
        ..Default::default()
    };
    o.insert(&db).await.unwrap();

    // CommissionLog yesterday
    let c = commission_log::ActiveModel {
        id: Set(1),
        invite_user_id: Set(2),
        user_id: Set(1),
        trade_no: Set("TRADE_YESTERDAY".to_string()),
        order_amount: Set(5000),
        get_amount: Set(1000),
        created_at: Set(yesterday_midday),
        updated_at: Set(yesterday_midday),
    };
    c.insert(&db).await.unwrap();

    // Registered User yesterday
    let mut u = create_test_user(1, "yesterday_user@test.com");
    u.invite_user_id = Set(Some(2));
    u.created_at = Set(yesterday_midday);
    u.updated_at = Set(yesterday_midday);
    u.insert(&db).await.unwrap();

    // Run record_daily_stats
    let stat = record_daily_stats(&state).await.unwrap();
    assert_eq!(stat.order_count, 1);
    assert_eq!(stat.order_total, 5000);
    assert_eq!(stat.paid_count, 1);
    assert_eq!(stat.paid_total, 5000);
    assert_eq!(stat.commission_count, 1);
    assert_eq!(stat.commission_total, 1000);
    assert_eq!(stat.register_count, 1);
    assert_eq!(stat.invite_count, 1);

    // Running again updates the same record
    let updated_stat = record_daily_stats(&state).await.unwrap();
    assert_eq!(updated_stat.id, stat.id);
    assert_eq!(Stat::find().count(&db).await.unwrap(), 1);
}

#[tokio::test]
async fn test_node_checker_worker() {
    let db = setup_phase10_db().await;
    let state = create_test_state(db.clone()).await;

    let pruned = check_and_cleanup_nodes(&state).await.unwrap();
    assert_eq!(pruned, 0);
}
