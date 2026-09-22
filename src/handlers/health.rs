use crate::common::ApiResponse;
use axum::response::IntoResponse;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthStatus {
    pub status: &'static str,
    pub version: &'static str,
    pub app: &'static str,
}

pub async fn health_check() -> impl IntoResponse {
    let status = HealthStatus {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        app: "Xboard-RS",
    };
    ApiResponse::success(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_payload() {
        let status = HealthStatus {
            status: "ok",
            version: "0.1.0",
            app: "Xboard-RS",
        };
        let resp = ApiResponse::success(status);
        assert_eq!(resp.status.as_deref(), Some("success"));
        assert_eq!(resp.data.status, "ok");
    }
}
