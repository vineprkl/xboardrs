use axum::{
    body::to_bytes,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;
use xboard_rs::{
    common::{ApiResponse, AppError, PaginatedResponse, PaginationQuery},
    config::AppConfig,
    handlers::app_router,
};

#[tokio::test]
async fn test_health_endpoint() {
    let app = app_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "success");
    assert_eq!(json["message"], "操作成功");
    assert_eq!(json["data"]["status"], "ok");
    assert_eq!(json["data"]["app"], "Xboard-RS");
}

#[test]
fn test_api_response_variants() {
    // 1. Success variant
    let s = ApiResponse::success("payload");
    let v_s: Value = serde_json::to_value(&s).unwrap();
    assert_eq!(v_s["status"], "success");
    assert_eq!(v_s["message"], "操作成功");
    assert_eq!(v_s["data"], "payload");

    // 2. Data-only variant
    let d = ApiResponse::data(vec![1, 2, 3]);
    let v_d: Value = serde_json::to_value(&d).unwrap();
    assert!(v_d.get("status").is_none());
    assert!(v_d.get("message").is_none());
    assert_eq!(v_d["data"], serde_json::json!([1, 2, 3]));

    // 3. Custom message
    let c = ApiResponse::with_message(true, "更新成功");
    let v_c: Value = serde_json::to_value(&c).unwrap();
    assert_eq!(v_c["status"], "success");
    assert_eq!(v_c["message"], "更新成功");
    assert_eq!(v_c["data"], true);
}

#[tokio::test]
async fn test_app_error_response_transformation() {
    use axum::response::IntoResponse;

    // Bad Request
    let err = AppError::BadRequest("参数错误: email 格式不合法".into());
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["status"], "fail");
    assert_eq!(json["message"], "参数错误: email 格式不合法");

    // Unauthorized
    let err = AppError::Unauthorized("授权失败，请先登录".into());
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["status"], "fail");
    assert_eq!(json["message"], "授权失败，请先登录");

    // Validation
    let details = serde_json::json!({
        "email": ["The email must be a valid email address."]
    });
    let err = AppError::Validation("验证失败".into(), Some(details));
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["status"], "fail");
    assert_eq!(json["message"], "验证失败");
    assert!(json["errors"]["email"].is_array());
}

#[test]
fn test_paginated_response_math() {
    let list = vec!["item1", "item2"];
    let page = PaginatedResponse::new(list, 95, 10, 10);
    assert_eq!(page.total, 95);
    assert_eq!(page.current_page, 10);
    assert_eq!(page.per_page, 10);
    assert_eq!(page.last_page, 10);

    let query = PaginationQuery {
        current: 10,
        page_size: 10,
        sort_type: Some("desc".into()),
        sort_field: Some("id".into()),
    };
    assert_eq!(query.offset(), 90);
    assert_eq!(query.limit(), 10);
}

#[test]
fn test_app_config_loading() {
    let cfg = AppConfig::from_env();
    assert_eq!(cfg.server_port, 7001);
    assert_eq!(cfg.app_name, "Xboard");
}
