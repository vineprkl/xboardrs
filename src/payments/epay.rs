use super::{NotifyResult, PayPayload, PayResult, PaymentGateway};
use crate::common::AppError;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub struct EpayGateway {
    pub url: String,
    pub pid: String,
    pub key: String,
    pub pay_type: Option<String>,
}

impl EpayGateway {
    pub fn new(config: &Value) -> Result<Self, AppError> {
        let url = config
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        let pid = config
            .get("pid")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let key = config
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let pay_type = config
            .get("type")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        if url.is_empty() || pid.is_empty() || key.is_empty() {
            return Err(AppError::Custom(
                400,
                "EPay configuration is missing url, pid or key".to_string(),
            ));
        }

        Ok(Self {
            url,
            pid,
            key,
            pay_type,
        })
    }
}

#[async_trait]
impl PaymentGateway for EpayGateway {
    async fn pay(&self, payload: &PayPayload) -> Result<PayResult, AppError> {
        let money = format!("{:.2}", (payload.total_amount as f64) / 100.0);
        let mut params = BTreeMap::new();
        params.insert("money", money);
        params.insert("name", payload.trade_no.clone());
        params.insert("notify_url", payload.notify_url.clone());
        params.insert("return_url", payload.return_url.clone());
        params.insert("out_trade_no", payload.trade_no.clone());
        params.insert("pid", self.pid.clone());
        if let Some(ref t) = self.pay_type {
            if !t.is_empty() {
                params.insert("type", t.clone());
            }
        }

        let mut query_parts = Vec::new();
        for (k, v) in &params {
            query_parts.push(format!("{}={}", k, v));
        }
        let query_str = query_parts.join("&");
        let sign_str = format!("{}{}", query_str, self.key);
        let sign = format!("{:x}", md5::compute(sign_str.as_bytes()));

        let submit_url = format!(
            "{}/submit.php?{}&sign={}&sign_type=MD5",
            self.url, query_str, sign
        );

        Ok(PayResult {
            pay_type: 1,
            data: Value::String(submit_url),
        })
    }

    async fn notify(&self, params: &HashMap<String, String>) -> Result<NotifyResult, AppError> {
        let sign = params
            .get("sign")
            .ok_or_else(|| AppError::Custom(422, "Missing sign in callback".to_string()))?;

        let mut sorted_params = BTreeMap::new();
        for (k, v) in params {
            if k != "sign" && k != "sign_type" {
                sorted_params.insert(k.as_str(), v.as_str());
            }
        }

        let mut query_parts = Vec::new();
        for (k, v) in &sorted_params {
            query_parts.push(format!("{}={}", k, v));
        }
        let query_str = query_parts.join("&");
        let sign_str = format!("{}{}", query_str, self.key);
        let calculated_sign = format!("{:x}", md5::compute(sign_str.as_bytes()));

        if !calculated_sign.eq_ignore_ascii_case(sign) {
            return Err(AppError::Custom(422, "verify error".to_string()));
        }

        let out_trade_no = sorted_params
            .get("out_trade_no")
            .copied()
            .or_else(|| sorted_params.get("trade_no").copied())
            .unwrap_or_default();
        let callback_no = sorted_params.get("trade_no").copied().unwrap_or_default();

        Ok(NotifyResult {
            trade_no: out_trade_no.to_string(),
            callback_no: callback_no.to_string(),
            custom_result: Some("success".to_string()),
        })
    }
}
