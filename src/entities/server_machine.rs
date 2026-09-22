use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "v2_server_machine")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub token: String,
    pub notes: Option<String>,
    pub is_active: bool,
    pub last_seen_at: Option<i64>,
    pub load_status: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::server::Entity")]
    Server,
    #[sea_orm(has_many = "super::server_machine_load_history::Entity")]
    LoadHistory,
}

impl Related<super::server::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Server.def()
    }
}

impl Related<super::server_machine_load_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::LoadHistory.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
