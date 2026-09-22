use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_current")]
    pub current: u64,
    #[serde(default = "default_page_size")]
    pub page_size: u64,
    pub sort_type: Option<String>,
    pub sort_field: Option<String>,
}

fn default_current() -> u64 {
    1
}

fn default_page_size() -> u64 {
    10
}

impl PaginationQuery {
    pub fn offset(&self) -> u64 {
        if self.current <= 1 {
            0
        } else {
            (self.current - 1) * self.page_size
        }
    }

    pub fn limit(&self) -> u64 {
        self.page_size.clamp(1, 100)
    }
}

impl Default for PaginationQuery {
    fn default() -> Self {
        Self {
            current: 1,
            page_size: 10,
            sort_type: None,
            sort_field: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_offset_and_limit() {
        let p1 = PaginationQuery {
            current: 1,
            page_size: 20,
            sort_type: None,
            sort_field: None,
        };
        assert_eq!(p1.offset(), 0);
        assert_eq!(p1.limit(), 20);

        let p2 = PaginationQuery {
            current: 3,
            page_size: 15,
            sort_type: None,
            sort_field: None,
        };
        assert_eq!(p2.offset(), 30);
        assert_eq!(p2.limit(), 15);

        let p3 = PaginationQuery {
            current: 1,
            page_size: 500, // should clamp to 100
            sort_type: None,
            sort_field: None,
        };
        assert_eq!(p3.limit(), 100);
    }
}
