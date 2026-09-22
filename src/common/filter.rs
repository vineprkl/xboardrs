use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Single filter condition from React Admin table.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FilterItem {
    pub id: String,
    pub value: Value,
    #[serde(default)]
    pub logic: Option<String>,
}

/// Single sort rule from React Admin table.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SortItem {
    pub id: String,
    #[serde(default)]
    pub desc: bool,
}

/// Dynamic query parameters sent by React Admin tables (GET queries or POST body).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdminTableQuery {
    pub current: Option<u64>,
    pub page: Option<u64>,
    #[serde(alias = "pageSize", alias = "per_page")]
    pub page_size: Option<u64>,
    pub filter: Option<Value>,
    pub sort: Option<Value>,
    pub is_commission: Option<bool>,
    pub id: Option<i32>,
}

impl AdminTableQuery {
    pub fn page(&self) -> u64 {
        self.current.or(self.page).unwrap_or(1).max(1)
    }

    pub fn per_page(&self) -> u64 {
        self.page_size.unwrap_or(10).clamp(1, 1000)
    }

    pub fn offset(&self) -> u64 {
        (self.page() - 1) * self.per_page()
    }

    pub fn filters(&self) -> Vec<FilterItem> {
        match &self.filter {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect(),
            Some(Value::String(s)) => {
                serde_json::from_str::<Vec<FilterItem>>(s).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    pub fn sorts(&self) -> Vec<SortItem> {
        match &self.sort {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect(),
            Some(Value::String(s)) => serde_json::from_str::<Vec<SortItem>>(s).unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}
