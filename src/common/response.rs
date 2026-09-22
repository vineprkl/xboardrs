use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

/// Standard API envelope matching Xboard frontend contracts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiResponse<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

impl<T> ApiResponse<T> {
    /// Creates a success response with "success" status and standard success message.
    pub fn success(data: T) -> Self {
        Self {
            status: Some("success".to_string()),
            message: Some("操作成功".to_string()),
            data,
            error: None,
        }
    }

    /// Creates a response containing only the data payload.
    pub fn data(data: T) -> Self {
        Self {
            status: None,
            message: None,
            data,
            error: None,
        }
    }

    /// Creates a success response with a custom message.
    pub fn with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            status: Some("success".to_string()),
            message: Some(message.into()),
            data,
            error: None,
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

/// Paginated data structure compatible with Laravel's LengthAwarePaginator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaginatedResponse<T> {
    pub total: u64,
    pub current_page: u64,
    pub per_page: u64,
    pub last_page: u64,
    pub data: Vec<T>,
}

impl<T> PaginatedResponse<T> {
    pub fn new(data: Vec<T>, total: u64, current_page: u64, per_page: u64) -> Self {
        let last_page = if per_page == 0 {
            1
        } else {
            total.div_ceil(per_page)
        };
        Self {
            total,
            current_page,
            per_page,
            last_page: last_page.max(1),
            data,
        }
    }
}

impl<T: Serialize> IntoResponse for PaginatedResponse<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_serialization() {
        let resp = ApiResponse::success("hello");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""status":"success""#));
        assert!(json.contains(r#""message":"操作成功""#));
        assert!(json.contains(r#""data":"hello""#));
        assert!(!json.contains(r#""error""#));
    }

    #[test]
    fn test_data_only_serialization() {
        let resp = ApiResponse::data(42);
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"data":42}"#);
    }

    #[test]
    fn test_paginated_response() {
        let items = vec!["a", "b", "c"];
        let page = PaginatedResponse::new(items, 10, 1, 3);
        assert_eq!(page.total, 10);
        assert_eq!(page.current_page, 1);
        assert_eq!(page.per_page, 3);
        assert_eq!(page.last_page, 4);

        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains(r#""total":10"#));
        assert!(json.contains(r#""last_page":4"#));
        assert!(json.contains(r#""data":["a","b","c"]"#));
    }
}
