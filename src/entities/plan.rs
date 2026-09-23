use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "v2_plan")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub group_id: i32,
    pub transfer_enable: i64,
    pub name: String,
    pub speed_limit: Option<i32>,
    pub show: bool,
    pub sort: Option<i32>,
    pub renew: bool,
    pub sell: Option<bool>,
    pub prices: Option<String>,
    pub content: Option<String>,
    pub month_price: Option<i32>,
    pub quarter_price: Option<i32>,
    pub half_year_price: Option<i32>,
    pub year_price: Option<i32>,
    pub two_year_price: Option<i32>,
    pub three_year_price: Option<i32>,
    pub onetime_price: Option<i32>,
    pub reset_price: Option<i32>,
    pub reset_traffic_method: Option<i32>,
    pub capacity_limit: Option<i32>,
    pub device_limit: Option<i32>,
    pub tags: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Model {
    pub fn get_price(&self, period: &str) -> Option<i32> {
        let period_key = match period {
            "month_price" | "monthly" => "monthly",
            "quarter_price" | "quarterly" => "quarterly",
            "half_year_price" | "half_yearly" => "half_yearly",
            "year_price" | "yearly" => "yearly",
            "two_year_price" | "two_yearly" => "two_yearly",
            "three_year_price" | "three_yearly" => "three_yearly",
            "onetime_price" | "onetime" => "onetime",
            "reset_price" | "reset_traffic" => "reset_traffic",
            other => other,
        };

        // 1. Try prices JSON
        if let Some(ref prices_str) = self.prices {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(prices_str) {
                let parsed_p = val.get(period_key).and_then(|v| match v {
                    serde_json::Value::Number(n) => n.as_f64(),
                    serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
                    _ => None,
                });
                if let Some(p) = parsed_p {
                    return Some((p * 100.0).round() as i32);
                }
            }
        }

        // 2. Fallback to legacy price columns
        match period_key {
            "monthly" => self.month_price,
            "quarterly" => self.quarter_price,
            "half_yearly" => self.half_year_price,
            "yearly" => self.year_price,
            "two_yearly" => self.two_year_price,
            "three_yearly" => self.three_year_price,
            "onetime" => self.onetime_price,
            "reset_traffic" => self.reset_price,
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::user::Entity")]
    User,
    #[sea_orm(has_many = "super::order::Entity")]
    Order,
    #[sea_orm(
        belongs_to = "super::server_group::Entity",
        from = "Column::GroupId",
        to = "super::server_group::Column::Id"
    )]
    Group,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::order::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Order.def()
    }
}

impl Related<super::server_group::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
