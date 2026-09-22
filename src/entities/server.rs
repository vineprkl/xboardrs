use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "v2_server")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub r#type: String,
    pub code: Option<String>,
    pub parent_id: Option<i32>,
    pub machine_id: Option<i32>,
    pub group_ids: Option<String>,
    pub route_ids: Option<String>,
    pub tags: Option<String>,
    pub host: String,
    pub port: String,
    pub server_port: i32,
    pub rate: f64,
    pub rate_time_enable: bool,
    pub rate_time_ranges: Option<String>,
    pub protocol_settings: Option<String>,
    pub custom_outbounds: Option<String>,
    pub custom_routes: Option<String>,
    pub cert_config: Option<String>,
    pub show: bool,
    pub enabled: Option<bool>,
    pub sort: Option<i32>,
    pub transfer_enable: Option<i64>,
    pub u: Option<i64>,
    pub d: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::server_machine::Entity",
        from = "Column::MachineId",
        to = "super::server_machine::Column::Id"
    )]
    Machine,
    #[sea_orm(belongs_to = "Entity", from = "Column::ParentId", to = "Column::Id")]
    Parent,
}

impl Related<super::server_machine::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Machine.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
