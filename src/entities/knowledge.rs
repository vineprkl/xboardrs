use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "v2_knowledge")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub language: String,
    pub category: String,
    pub title: String,
    pub body: String,
    pub sort: Option<i32>,
    pub show: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
