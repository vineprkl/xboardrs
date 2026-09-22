use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "v2_user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub invite_user_id: Option<i32>,
    pub telegram_id: Option<i64>,
    pub email: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub password_algo: Option<String>,
    pub password_salt: Option<String>,
    pub balance: i32,
    pub discount: Option<i32>,
    pub commission_type: i8,
    pub commission_rate: Option<f64>,
    pub commission_balance: i32,
    pub t: i32,
    pub u: i64,
    pub d: i64,
    pub transfer_enable: i64,
    pub banned: bool,
    pub is_admin: bool,
    pub is_staff: bool,
    pub last_login_at: Option<i64>,
    pub last_login_ip: Option<i32>,
    pub uuid: String,
    pub group_id: Option<i32>,
    pub plan_id: Option<i32>,
    pub speed_limit: Option<i32>,
    pub remind_expire: Option<bool>,
    pub remind_traffic: Option<bool>,
    pub token: String,
    pub expired_at: Option<i64>,
    pub remarks: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub device_limit: Option<i32>,
    pub parent_id: Option<i32>,
    pub next_reset_at: Option<i64>,
    pub last_reset_at: Option<i64>,
    pub reset_count: Option<i32>,
    pub commission_auto_check: Option<bool>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::plan::Entity",
        from = "Column::PlanId",
        to = "super::plan::Column::Id"
    )]
    Plan,
    #[sea_orm(
        belongs_to = "super::server_group::Entity",
        from = "Column::GroupId",
        to = "super::server_group::Column::Id"
    )]
    Group,
}

impl Related<super::plan::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Plan.def()
    }
}

impl Related<super::server_group::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
