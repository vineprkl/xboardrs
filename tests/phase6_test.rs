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
        invite_code, plan, server_group, user, InviteCode, PersonalAccessToken, Plan, Server,
        ServerGroup, ServerMachine, ServerMachineLoadHistory, ServerRoute, Setting, User,
    },
    services::{
        AuthService, DeviceStateService, PlanService, RegisterDto, ServerService, SettingService,
        UserService,
    },
    utils::verify_password,
};

async fn setup_phase6_db() -> DatabaseConnection {
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

async fn insert_dummy_user(db: &DatabaseConnection, id: i32, email: &str) {
    let now = chrono::Utc::now().timestamp();
    let u = user::ActiveModel {
        id: Set(id),
        email: Set(email.to_string()),
        password: Set("dummy".to_string()),
        balance: Set(0),
        commission_type: Set(0),
        commission_balance: Set(0),
        t: Set(0),
        u: Set(0),
        d: Set(0),
        transfer_enable: Set(10000),
        banned: Set(false),
        is_admin: Set(false),
        is_staff: Set(false),
        uuid: Set(format!("uuid-{}", id)),
        token: Set(format!("token-{}", id)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    u.insert(db).await.unwrap();
}

#[tokio::test]
async fn test_passport_register_success_and_validations() {
    let db = setup_phase6_db().await;
    let state = create_test_state(db.clone()).await;
    let app = app_router_with_state(state.clone());

    // 1. Successful Registration
    let req = Request::builder()
        .uri("/api/v1/passport/auth/register")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "newuser@example.com",
                "password": "strongpassword123"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json_res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json_res["status"], "success");
    let data = &json_res["data"];
    assert!(data["token"].as_str().unwrap().len() >= 32);
    assert!(data["auth_data"].as_str().unwrap().starts_with("Bearer "));
    assert_eq!(data["is_admin"], false);

    // Verify DB record
    let u = User::find()
        .filter(user::Column::Email.eq("newuser@example.com"))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(verify_password("strongpassword123", &u.password));

    // 2. Duplicate email rejected
    let req = Request::builder()
        .uri("/api/v1/passport/auth/register")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "newuser@example.com",
                "password": "strongpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 3. Validation: Empty email -> 422
    let req = Request::builder()
        .uri("/api/v1/passport/auth/register")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "",
                "password": "strongpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 4. Validation: Invalid email -> 422
    let req = Request::builder()
        .uri("/api/v1/passport/auth/register")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "notanemail",
                "password": "strongpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 5. Validation: Short password (< 8 chars) -> 422
    let req = Request::builder()
        .uri("/api/v1/passport/auth/register")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "user2@example.com",
                "password": "123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 6. Whitelist restriction
    state
        .setting_service
        .set("email_whitelist_enable", "1")
        .await
        .unwrap();
    state
        .setting_service
        .set("email_whitelist_suffix", "gmail.com,qq.com")
        .await
        .unwrap();

    let req = Request::builder()
        .uri("/api/v1/passport/auth/register")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "user@yahoo.com",
                "password": "strongpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = Request::builder()
        .uri("/api/v1/passport/auth/register")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "user@gmail.com",
                "password": "strongpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 7. Stop registration
    state
        .setting_service
        .set("stop_register", "1")
        .await
        .unwrap();
    let req = Request::builder()
        .uri("/api/v1/passport/auth/register")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "user3@gmail.com",
                "password": "strongpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_passport_invite_and_trial_plan() {
    let db = setup_phase6_db().await;
    let state = create_test_state(db.clone()).await;

    let now = chrono::Utc::now().timestamp();

    // 1. Create ServerGroup and Plan for trial
    let grp = server_group::ActiveModel {
        id: Set(1),
        name: Set("Trial Group".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    grp.insert(&db).await.unwrap();

    let p = plan::ActiveModel {
        id: Set(5),
        group_id: Set(1),
        transfer_enable: Set(10), // 10 GB
        name: Set("Trial Plan".to_string()),
        speed_limit: Set(Some(100)),
        show: Set(true),
        renew: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    p.insert(&db).await.unwrap();

    state
        .setting_service
        .set("try_out_plan_id", "5")
        .await
        .unwrap();
    state
        .setting_service
        .set("try_out_hour", "48")
        .await
        .unwrap();

    // 2. Insert inviter user 88 and Create InviteCode
    insert_dummy_user(&db, 88, "inviter88@test.com").await;
    let inv = invite_code::ActiveModel {
        id: Set(1),
        user_id: Set(88),
        code: Set("INVITE_XYZ".to_string()),
        status: Set(false),
        pv: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
    };
    inv.insert(&db).await.unwrap();

    let app = app_router_with_state(state.clone());

    // Register with invite code
    let req = Request::builder()
        .uri("/api/v1/passport/auth/register")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "invited@example.com",
                "password": "password123456",
                "invite_code": "INVITE_XYZ"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify user in DB
    let user = User::find()
        .filter(user::Column::Email.eq("invited@example.com"))
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(user.invite_user_id, Some(88));
    assert_eq!(user.plan_id, Some(5));
    assert_eq!(user.group_id, Some(1));
    assert_eq!(user.transfer_enable, 10 * 1073741824);
    assert!(user.expired_at.unwrap() >= now + 47 * 3600);

    // Verify invite code marked as used
    let inv_updated = InviteCode::find_by_id(1).one(&db).await.unwrap().unwrap();
    assert!(inv_updated.status);
}

#[tokio::test]
async fn test_passport_login_and_rate_limit() {
    let db = setup_phase6_db().await;
    let state = create_test_state(db.clone()).await;

    // Register user first
    let user = state
        .auth_service
        .register(RegisterDto {
            email: "login_user@test.com".to_string(),
            password: "correctpassword".to_string(),
            invite_code: None,
            email_code: None,
        })
        .await
        .unwrap();

    let app = app_router_with_state(state.clone());

    // 1. Successful login
    let req = Request::builder()
        .uri("/api/v1/passport/auth/login")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "login_user@test.com",
                "password": "correctpassword"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json_res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json_res["status"], "success");
    let bearer = json_res["data"]["auth_data"].as_str().unwrap();
    assert!(bearer.starts_with("Bearer "));

    // 1.1 Successful login with application/x-www-form-urlencoded (sent by umi.js frontend)
    let req_form = Request::builder()
        .uri("/api/v1/passport/auth/login")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(
            "email=login_user%40test.com&password=correctpassword",
        ))
        .unwrap();

    let resp_form = app.clone().oneshot(req_form).await.unwrap();
    assert_eq!(resp_form.status(), StatusCode::OK);
    let body_form = to_bytes(resp_form.into_body(), 1024 * 1024).await.unwrap();
    let json_res_form: Value = serde_json::from_slice(&body_form).unwrap();
    assert_eq!(json_res_form["status"], "success");

    // 2. Wrong password -> 400
    let req = Request::builder()
        .uri("/api/v1/passport/auth/login")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "login_user@test.com",
                "password": "wrongpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 3. Banned user -> 400
    let mut active: user::ActiveModel = user.into();
    active.banned = Set(true);
    active.update(&db).await.unwrap();

    let req = Request::builder()
        .uri("/api/v1/passport/auth/login")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "login_user@test.com",
                "password": "correctpassword"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 4. Rate limit on password errors
    state
        .setting_service
        .set("password_limit_enable", "1")
        .await
        .unwrap();
    state
        .setting_service
        .set("password_limit_count", "2")
        .await
        .unwrap();

    // Register second user for rate limit test
    state
        .auth_service
        .register(RegisterDto {
            email: "limit_user@test.com".to_string(),
            password: "validpassword".to_string(),
            invite_code: None,
            email_code: None,
        })
        .await
        .unwrap();

    // Attempt 1: fail
    let req = Request::builder()
        .uri("/api/v1/passport/auth/login")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({ "email": "limit_user@test.com", "password": "badpassword1" }).to_string(),
        ))
        .unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    // Attempt 2: fail
    let req = Request::builder()
        .uri("/api/v1/passport/auth/login")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({ "email": "limit_user@test.com", "password": "badpassword2" }).to_string(),
        ))
        .unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    // Attempt 3: blocked by rate limit -> 429
    let req = Request::builder()
        .uri("/api/v1/passport/auth/login")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({ "email": "limit_user@test.com", "password": "validpassword" }).to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_passport_email_verify_and_forget_password() {
    let db = setup_phase6_db().await;
    let state = create_test_state(db.clone()).await;

    // Register user
    state
        .auth_service
        .register(RegisterDto {
            email: "reset_user@test.com".to_string(),
            password: "oldpassword123".to_string(),
            invite_code: None,
            email_code: None,
        })
        .await
        .unwrap();

    let app = app_router_with_state(state.clone());

    // 1. Send email verify
    let req = Request::builder()
        .uri("/api/v1/passport/comm/sendEmailVerify")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({ "email": "reset_user@test.com" }).to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Cooldown prevents immediate re-send -> 400
    let req = Request::builder()
        .uri("/api/v1/passport/comm/sendEmailVerify")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({ "email": "reset_user@test.com" }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Get cached verify code
    let verify_code = state
        .auth_service
        .get_cache("EMAIL_VERIFY_CODE:reset_user@test.com")
        .await
        .unwrap();
    assert_eq!(verify_code.len(), 6);

    // 3. Reset password with wrong code -> 400
    let req = Request::builder()
        .uri("/api/v1/passport/auth/forget")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "reset_user@test.com",
                "email_code": "000000",
                "password": "newpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 4. Reset password with correct code -> 200
    let req = Request::builder()
        .uri("/api/v1/passport/auth/forget")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "reset_user@test.com",
                "email_code": verify_code,
                "password": "newpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 5. Login with new password -> 200
    let req = Request::builder()
        .uri("/api/v1/passport/auth/login")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "reset_user@test.com",
                "password": "newpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Login with old password -> 400
    let req = Request::builder()
        .uri("/api/v1/passport/auth/login")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({
                "email": "reset_user@test.com",
                "password": "oldpassword123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_passport_quick_login_and_token2login() {
    let db = setup_phase6_db().await;
    let state = create_test_state(db.clone()).await;

    // Register and login user
    let user = state
        .auth_service
        .register(RegisterDto {
            email: "quick_user@test.com".to_string(),
            password: "password12345".to_string(),
            invite_code: None,
            email_code: None,
        })
        .await
        .unwrap();

    let auth_data = state.auth_service.generate_auth_data(&user).await.unwrap();
    let app = app_router_with_state(state.clone());

    // 1. Get quick login URL using Authorization header
    let req = Request::builder()
        .uri("/api/v1/passport/auth/getQuickLoginUrl")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, &auth_data.auth_data)
        .body(axum::body::Body::from(
            json!({ "redirect": "sub" }).to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json_res: Value = serde_json::from_slice(&body).unwrap();
    let quick_url = json_res["data"].as_str().unwrap();
    assert!(quick_url.contains("/#/login?verify="));
    assert!(quick_url.contains("&redirect=sub"));

    // Extract verify code
    let verify_code = quick_url
        .split("verify=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();

    // 2. Exchange verify code via token2Login
    let req = Request::builder()
        .uri(format!(
            "/api/v1/passport/auth/token2Login?verify={}",
            verify_code
        ))
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let json_res: Value = serde_json::from_slice(&body).unwrap();
    let exchanged_auth = &json_res["data"];
    assert!(exchanged_auth["auth_data"]
        .as_str()
        .unwrap()
        .starts_with("Bearer "));

    // 3. Verify single-use token: second exchange -> 400
    let req = Request::builder()
        .uri(format!(
            "/api/v1/passport/auth/token2Login?verify={}",
            verify_code
        ))
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 4. Token redirect case
    let req = Request::builder()
        .uri("/api/v1/passport/auth/token2Login?token=my_session_token&redirect=profile")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/#/login?verify=my_session_token&redirect=profile"));
}

#[tokio::test]
async fn test_passport_invite_pv() {
    let db = setup_phase6_db().await;
    let state = create_test_state(db.clone()).await;

    let now = chrono::Utc::now().timestamp();

    insert_dummy_user(&db, 1, "inviter1@test.com").await;

    // Create InviteCode
    let inv = invite_code::ActiveModel {
        id: Set(10),
        user_id: Set(1),
        code: Set("TRACK_PV".to_string()),
        status: Set(false),
        pv: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
    };
    inv.insert(&db).await.unwrap();

    let app = app_router_with_state(state.clone());

    let req = Request::builder()
        .uri("/api/v1/passport/comm/pv")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            json!({ "invite_code": "TRACK_PV" }).to_string(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify PV incremented
    let record = InviteCode::find_by_id(10).one(&db).await.unwrap().unwrap();
    assert_eq!(record.pv, 1);
}
