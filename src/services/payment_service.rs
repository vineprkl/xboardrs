use crate::{
    common::AppError,
    entities::{payment, Payment},
    payments::{create_gateway, NotifyResult, PayPayload, PayResult},
    services::SettingService,
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMethodDto {
    pub id: i32,
    pub name: String,
    pub payment: String,
    pub icon: Option<String>,
    pub handling_fee_fixed: Option<i32>,
    pub handling_fee_percent: Option<f64>,
}

#[derive(Clone)]
pub struct PaymentService {
    db: DatabaseConnection,
    setting_service: SettingService,
}

impl PaymentService {
    pub fn new(db: DatabaseConnection, setting_service: SettingService) -> Self {
        Self {
            db,
            setting_service,
        }
    }

    /// Fetches all active payment methods for the user.
    pub async fn get_payment_methods(&self) -> Result<Vec<PaymentMethodDto>, AppError> {
        let payments = Payment::find()
            .filter(payment::Column::Enable.eq(true))
            .order_by_asc(payment::Column::Sort)
            .all(&self.db)
            .await?;

        let list = payments
            .into_iter()
            .map(|p| PaymentMethodDto {
                id: p.id,
                name: p.name,
                payment: p.payment,
                icon: p.icon,
                handling_fee_fixed: p.handling_fee_fixed,
                handling_fee_percent: p.handling_fee_percent,
            })
            .collect();

        Ok(list)
    }

    /// Initiates a payment for an order through a configured payment gateway.
    pub async fn pay(
        &self,
        payment_id: i32,
        trade_no: &str,
        total_amount: i32,
        user_id: i32,
        stripe_token: Option<String>,
    ) -> Result<PayResult, AppError> {
        let payment_model = Payment::find_by_id(payment_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::Custom(400, "Payment method is not available".to_string()))?;

        if !payment_model.enable {
            return Err(AppError::Custom(
                400,
                "Payment method is not available".to_string(),
            ));
        }

        let config: Value = serde_json::from_str(&payment_model.config)
            .map_err(|e| AppError::Custom(500, format!("Invalid payment config: {}", e)))?;

        let app_url = self
            .setting_service
            .get_string("app_url", "http://localhost")
            .await
            .trim_end_matches('/')
            .to_string();

        let base_notify = if let Some(ref nd) = payment_model.notify_domain {
            if !nd.trim().is_empty() {
                nd.trim().trim_end_matches('/').to_string()
            } else {
                app_url.clone()
            }
        } else {
            app_url.clone()
        };

        let notify_url = format!(
            "{}/api/v1/guest/payment/notify/{}/{}",
            base_notify, payment_model.payment, payment_model.uuid
        );
        let return_url = format!("{}/#/order/{}", app_url, trade_no);

        let gateway = create_gateway(&payment_model.payment, &config)?;

        let payload = PayPayload {
            trade_no: trade_no.to_string(),
            total_amount,
            notify_url,
            return_url,
            user_id,
            stripe_token,
        };

        gateway.pay(&payload).await
    }

    /// Verifies and processes an incoming payment webhook / notification.
    pub async fn notify(
        &self,
        method: &str,
        uuid: &str,
        params: &HashMap<String, String>,
    ) -> Result<NotifyResult, AppError> {
        let payment_model = Payment::find()
            .filter(payment::Column::Uuid.eq(uuid))
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::Custom(404, "Payment method not found".to_string()))?;

        if !payment_model.enable {
            return Err(AppError::Custom(400, "gate is not enable".to_string()));
        }

        let config: Value = serde_json::from_str(&payment_model.config)
            .map_err(|e| AppError::Custom(500, format!("Invalid payment config: {}", e)))?;

        let gateway = create_gateway(method, &config)?;

        gateway.notify(params).await
    }
}
