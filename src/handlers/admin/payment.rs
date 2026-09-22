use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, Set, TransactionTrait};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    common::{ApiResponse, AppError, AppState},
    entities::{payment, Payment},
    handlers::auth::AuthenticatedAdmin,
    utils::generate_uuid,
};

#[derive(Debug, Deserialize)]
pub struct PaymentSaveRequest {
    pub id: Option<i32>,
    pub name: String,
    pub payment: String,
    pub icon: Option<String>,
    pub config: Value,
    pub notify_domain: Option<String>,
    pub handling_fee_fixed: Option<i32>,
    pub handling_fee_percent: Option<f64>,
    pub enable: Option<bool>,
    pub sort: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct PaymentIdRequest {
    pub id: i32,
}

#[derive(Debug, Deserialize)]
pub struct PaymentSortRequest {
    pub ids: Vec<i32>,
}

/// GET /api/v2/admin/payment/getPaymentMethods
pub async fn get_payment_methods(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    let methods = vec!["Epay", "AlipayF2f", "Mgate", "Stripe"];
    Ok(ApiResponse::success(methods).into_response())
}

/// GET /api/v2/admin/payment/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
) -> Result<Response, AppError> {
    let payments = Payment::find()
        .order_by_asc(payment::Column::Sort)
        .all(&state.db)
        .await?;

    let app_url = state
        .setting_service
        .get_string("app_url", "http://127.0.0.1")
        .await;

    let mut list = Vec::new();
    for p in payments {
        let domain = p.notify_domain.as_deref().unwrap_or(&app_url);
        let notify_url = format!(
            "{}/api/v1/guest/payment/notify/{}/{}",
            domain.trim_end_matches('/'),
            p.payment,
            p.uuid
        );

        let mut val = serde_json::to_value(&p).unwrap_or_default();
        if let Some(obj) = val.as_object_mut() {
            obj.insert("notify_url".to_string(), json!(notify_url));
            if let Ok(cfg) = serde_json::from_str::<Value>(&p.config) {
                obj.insert("config".to_string(), cfg);
            }
        }
        list.push(val);
    }

    Ok(ApiResponse::success(list).into_response())
}

/// POST /api/v2/admin/payment/getPaymentForm
pub async fn get_payment_form(
    _admin: AuthenticatedAdmin,
    Json(payload): Json<Value>,
) -> Result<Response, AppError> {
    let payment_type = payload
        .get("payment")
        .and_then(|v| v.as_str())
        .unwrap_or("Epay");

    let form = match payment_type {
        "Epay" => json!({
            "url": { "label": "API地址", "type": "input", "placeholder": "https://epay.com" },
            "pid": { "label": "商户ID", "type": "input" },
            "key": { "label": "商户密钥", "type": "input" },
            "type": { "label": "支付方式", "type": "select", "options": ["alipay", "wxpay", "qqpay"] }
        }),
        "AlipayF2f" => json!({
            "app_id": { "label": "应用APPID", "type": "input" },
            "private_key": { "label": "应用私钥", "type": "textarea" },
            "public_key": { "label": "支付宝公钥", "type": "textarea" }
        }),
        "Stripe" => json!({
            "currency": { "label": "货币类型", "type": "input" },
            "stripe_sk": { "label": "Stripe Secret Key", "type": "input" },
            "stripe_pk": { "label": "Stripe Public Key", "type": "input" },
            "webhook_secret": { "label": "Stripe Webhook Secret", "type": "input" }
        }),
        _ => json!({
            "url": { "label": "接口URL", "type": "input" },
            "app_id": { "label": "APP ID", "type": "input" },
            "app_secret": { "label": "APP SECRET", "type": "input" }
        }),
    };

    Ok(ApiResponse::success(form).into_response())
}

/// POST /api/v2/admin/payment/save
pub async fn save(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<PaymentSaveRequest>,
) -> Result<Response, AppError> {
    let now = chrono::Utc::now().timestamp();
    let config_str = payload.config.to_string();

    if let Some(id) = payload.id {
        let p = Payment::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "支付方式不存在".to_string()))?;

        let mut p_active: payment::ActiveModel = p.into();
        p_active.name = Set(payload.name);
        p_active.payment = Set(payload.payment);
        if payload.icon.is_some() {
            p_active.icon = Set(payload.icon);
        }
        p_active.config = Set(config_str);
        if payload.notify_domain.is_some() {
            p_active.notify_domain = Set(payload.notify_domain);
        }
        if payload.handling_fee_fixed.is_some() {
            p_active.handling_fee_fixed = Set(payload.handling_fee_fixed);
        }
        if payload.handling_fee_percent.is_some() {
            p_active.handling_fee_percent = Set(payload.handling_fee_percent);
        }
        if let Some(en) = payload.enable {
            p_active.enable = Set(en);
        }
        if payload.sort.is_some() {
            p_active.sort = Set(payload.sort);
        }
        p_active.updated_at = Set(now);
        p_active.update(&state.db).await?;
    } else {
        let new_p = payment::ActiveModel {
            uuid: Set(generate_uuid()),
            payment: Set(payload.payment),
            name: Set(payload.name),
            icon: Set(payload.icon),
            config: Set(config_str),
            notify_domain: Set(payload.notify_domain),
            handling_fee_fixed: Set(payload.handling_fee_fixed),
            handling_fee_percent: Set(payload.handling_fee_percent),
            enable: Set(payload.enable.unwrap_or(true)),
            sort: Set(payload.sort),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_p.insert(&state.db).await?;
    }

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/payment/show
pub async fn show(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<PaymentIdRequest>,
) -> Result<Response, AppError> {
    let p = Payment::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "支付方式不存在".to_string()))?;

    let mut p_active: payment::ActiveModel = p.clone().into();
    p_active.enable = Set(!p.enable);
    p_active.updated_at = Set(chrono::Utc::now().timestamp());
    p_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/payment/drop
pub async fn drop(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<PaymentIdRequest>,
) -> Result<Response, AppError> {
    let p = Payment::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "支付方式不存在".to_string()))?;

    let p_active: payment::ActiveModel = p.into();
    p_active.delete(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/payment/sort
pub async fn sort(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<PaymentSortRequest>,
) -> Result<Response, AppError> {
    let txn = state.db.begin().await?;
    for (idx, id) in payload.ids.iter().enumerate() {
        if let Some(p) = Payment::find_by_id(*id).one(&txn).await? {
            let mut p_active: payment::ActiveModel = p.into();
            p_active.sort = Set(Some((idx + 1) as i32));
            p_active.update(&txn).await?;
        }
    }
    txn.commit().await?;

    Ok(ApiResponse::success(true).into_response())
}
