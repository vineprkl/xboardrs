use super::{NotifyResult, PayPayload, PayResult, PaymentGateway};
use crate::common::AppError;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub struct MgateGateway {
    pub url: String,
    pub app_id: String,
    pub app_secret: String,
    pub source_currency: Option<String>,
}

impl MgateGateway {
    pub fn new(config: &Value) -> Result<Self, AppError> {
        let url = config
            .get("mgate_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        let app_id = config
            .get("mgate_app_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let app_secret = config
            .get("mgate_app_secret")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let source_currency = config
            .get("mgate_source_currency")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        if url.is_empty() || app_id.is_empty() || app_secret.is_empty() {
            return Err(AppError::Custom(
                400,
                "MGate configuration is missing url, app_id or app_secret".to_string(),
            ));
        }

        Ok(Self {
            url,
            app_id,
            app_secret,
            source_currency,
        })
    }
}

#[async_trait]
impl PaymentGateway for MgateGateway {
    async fn pay(&self, payload: &PayPayload) -> Result<PayResult, AppError> {
        let mut params = BTreeMap::new();
        params.insert("out_trade_no".to_string(), payload.trade_no.clone());
        params.insert("total_amount".to_string(), payload.total_amount.to_string());
        params.insert("notify_url".to_string(), payload.notify_url.clone());
        params.insert("return_url".to_string(), payload.return_url.clone());
        params.insert("app_id".to_string(), self.app_id.clone());

        if let Some(ref cur) = self.source_currency {
            params.insert("source_currency".to_string(), cur.clone());
        }

        let mut query_parts = Vec::new();
        for (k, v) in &params {
            query_parts.push(format!("{}={}", k, v));
        }
        let query_str = query_parts.join("&");
        let sign_str = format!("{}{}", query_str, self.app_secret);
        let sign = format!("{:x}", md5::compute(sign_str.as_bytes()));
        params.insert("sign".to_string(), sign);

        // Perform HTTP request if url is live
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| AppError::Custom(500, e.to_string()))?;

        let fetch_url = format!("{}/v1/gateway/fetch", self.url);
        let resp = client
            .post(&fetch_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::Custom(500, format!("MGate network error: {}", e)))?;

        let json_val: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Custom(500, format!("MGate parse error: {}", e)))?;

        if let Some(pay_url) = json_val
            .get("data")
            .and_then(|d| d.get("pay_url"))
            .and_then(|u| u.as_str())
        {
            Ok(PayResult {
                pay_type: 1,
                data: Value::String(pay_url.to_string()),
            })
        } else {
            let msg = json_val
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("MGate request failed");
            Err(AppError::Custom(400, msg.to_string()))
        }
    }

    async fn notify(&self, params: &HashMap<String, String>) -> Result<NotifyResult, AppError> {
        let sign = params
            .get("sign")
            .ok_or_else(|| AppError::Custom(422, "Missing sign in callback".to_string()))?;

        let mut sorted_params = BTreeMap::new();
        for (k, v) in params {
            if k != "sign" {
                sorted_params.insert(k.as_str(), v.as_str());
            }
        }

        let mut query_parts = Vec::new();
        for (k, v) in &sorted_params {
            query_parts.push(format!("{}={}", k, v));
        }
        let query_str = query_parts.join("&");
        let sign_str = format!("{}{}", query_str, self.app_secret);
        let calculated_sign = format!("{:x}", md5::compute(sign_str.as_bytes()));

        if !calculated_sign.eq_ignore_ascii_case(sign) {
            return Err(AppError::Custom(422, "verify error".to_string()));
        }

        let out_trade_no = sorted_params
            .get("out_trade_no")
            .copied()
            .unwrap_or_default();
        let callback_no = sorted_params.get("trade_no").copied().unwrap_or_default();

        Ok(NotifyResult {
            trade_no: out_trade_no.to_string(),
            callback_no: callback_no.to_string(),
            custom_result: Some("success".to_string()),
        })
    }
}
