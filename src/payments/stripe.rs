use super::{NotifyResult, PayPayload, PayResult, PaymentGateway};
use crate::common::AppError;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

pub struct StripeGateway {
    pub stripe_sk: String,
    pub currency: String,
}

impl StripeGateway {
    pub fn new(config: &Value) -> Result<Self, AppError> {
        let stripe_sk = config
            .get("stripe_sk")
            .or_else(|| config.get("sk"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let currency = config
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("cny")
            .to_lowercase();

        if stripe_sk.is_empty() {
            return Err(AppError::Custom(
                400,
                "Stripe configuration missing secret key".to_string(),
            ));
        }

        Ok(Self {
            stripe_sk,
            currency,
        })
    }
}

#[async_trait]
impl PaymentGateway for StripeGateway {
    async fn pay(&self, payload: &PayPayload) -> Result<PayResult, AppError> {
        // In Stripe checkout mode, type 1 is redirect or type -1
        let redirect_url = format!(
            "https://checkout.stripe.com/pay/cs_{}_{}",
            self.currency, payload.trade_no
        );
        Ok(PayResult {
            pay_type: 1,
            data: Value::String(redirect_url),
        })
    }

    async fn notify(&self, params: &HashMap<String, String>) -> Result<NotifyResult, AppError> {
        let out_trade_no = params
            .get("out_trade_no")
            .or_else(|| params.get("trade_no"))
            .cloned()
            .unwrap_or_default();
        let callback_no = params
            .get("callback_no")
            .or_else(|| params.get("id"))
            .cloned()
            .unwrap_or_else(|| out_trade_no.clone());

        Ok(NotifyResult {
            trade_no: out_trade_no,
            callback_no,
            custom_result: Some("success".to_string()),
        })
    }
}
