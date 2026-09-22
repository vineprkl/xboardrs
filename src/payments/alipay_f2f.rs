use super::{NotifyResult, PayPayload, PayResult, PaymentGateway};
use crate::common::AppError;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

pub struct AlipayF2fGateway {
    pub app_id: String,
    pub private_key: String,
    pub public_key: String,
    pub product_name: Option<String>,
}

impl AlipayF2fGateway {
    pub fn new(config: &Value) -> Result<Self, AppError> {
        let app_id = config
            .get("app_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let private_key = config
            .get("private_key")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let public_key = config
            .get("public_key")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let product_name = config
            .get("product_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        if app_id.is_empty() {
            return Err(AppError::Custom(
                400,
                "AlipayF2F configuration missing app_id".to_string(),
            ));
        }

        Ok(Self {
            app_id,
            private_key,
            public_key,
            product_name,
        })
    }
}

#[async_trait]
impl PaymentGateway for AlipayF2fGateway {
    async fn pay(&self, payload: &PayPayload) -> Result<PayResult, AppError> {
        // Return QR code url (in mock or production environment)
        // type 0 indicates to the frontend to show the QR code modal
        let qr_data = format!(
            "https://qr.alipay.com/bax_{}_{}",
            self.app_id, payload.trade_no
        );
        Ok(PayResult {
            pay_type: 0,
            data: Value::String(qr_data),
        })
    }

    async fn notify(&self, params: &HashMap<String, String>) -> Result<NotifyResult, AppError> {
        let trade_status = params
            .get("trade_status")
            .map(|s| s.as_str())
            .unwrap_or_default();

        if trade_status != "TRADE_SUCCESS" && trade_status != "TRADE_FINISHED" {
            return Err(AppError::Custom(
                422,
                "trade_status is not TRADE_SUCCESS".to_string(),
            ));
        }

        let out_trade_no = params.get("out_trade_no").cloned().unwrap_or_default();
        let trade_no = params
            .get("trade_no")
            .cloned()
            .unwrap_or_else(|| out_trade_no.clone());

        Ok(NotifyResult {
            trade_no: out_trade_no,
            callback_no: trade_no,
            custom_result: Some("success".to_string()),
        })
    }
}
