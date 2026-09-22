pub mod alipay_f2f;
pub mod epay;
pub mod mgate;
pub mod stripe;

use crate::common::AppError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayPayload {
    pub trade_no: String,
    pub total_amount: i32, // In cents
    pub notify_url: String,
    pub return_url: String,
    pub user_id: i32,
    pub stripe_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayResult {
    /// 0 for QR code modal, 1 for redirect URL, -1 for directly completed
    #[serde(rename = "type")]
    pub pay_type: i8,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyResult {
    pub trade_no: String,
    pub callback_no: String,
    pub custom_result: Option<String>,
}

#[async_trait]
pub trait PaymentGateway: Send + Sync {
    async fn pay(&self, payload: &PayPayload) -> Result<PayResult, AppError>;
    async fn notify(&self, params: &HashMap<String, String>) -> Result<NotifyResult, AppError>;
}

pub fn create_gateway(
    payment_type: &str,
    config: &Value,
) -> Result<Box<dyn PaymentGateway>, AppError> {
    match payment_type.to_lowercase().as_str() {
        "epay" => Ok(Box::new(epay::EpayGateway::new(config)?)),
        "alipayf2f" | "alipay_f2f" => Ok(Box::new(alipay_f2f::AlipayF2fGateway::new(config)?)),
        "mgate" => Ok(Box::new(mgate::MgateGateway::new(config)?)),
        "stripe" | "stripe_checkout" => Ok(Box::new(stripe::StripeGateway::new(config)?)),
        _ => {
            // Default to EPay if config has standard epay keys (url, pid, key)
            if config.get("url").is_some() && config.get("pid").is_some() {
                Ok(Box::new(epay::EpayGateway::new(config)?))
            } else {
                Err(AppError::Custom(
                    400,
                    format!("Unsupported payment method: {}", payment_type),
                ))
            }
        }
    }
}
