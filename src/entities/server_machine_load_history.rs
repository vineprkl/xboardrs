use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "v2_server_machine_load_history")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub machine_id: i32,
    pub cpu: f32,
    pub mem_total: i64,
    pub mem_used: i64,
    pub disk_total: i64,
    pub disk_used: i64,
    pub recorded_at: i64,
    pub net_in_speed: Option<f64>,
    pub net_out_speed: Option<f64>,
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
}

impl Related<super::server_machine::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Machine.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
