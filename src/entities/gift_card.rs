pub mod template {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "v2_gift_card_template")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub name: String,
        pub description: Option<String>,
        pub r#type: i8,
        pub status: i8,
        pub conditions: Option<String>,
        pub rewards: String,
        pub limits: Option<String>,
        pub special_config: Option<String>,
        pub icon: Option<String>,
        pub background_image: Option<String>,
        pub theme_color: String,
        pub sort: i32,
        pub admin_id: i32,
        pub created_at: i64,
        pub updated_at: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::code::Entity")]
        Codes,
    }

    impl Related<super::code::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Codes.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod code {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "v2_gift_card_code")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub template_id: i32,
        pub code: String,
        pub batch_id: Option<String>,
        pub status: i8,
        pub user_id: Option<i32>,
        pub used_at: Option<i64>,
        pub expires_at: Option<i64>,
        pub actual_rewards: Option<String>,
        pub usage_count: i32,
        pub max_usage: i32,
        pub metadata: Option<String>,
        pub created_at: i64,
        pub updated_at: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::template::Entity",
            from = "Column::TemplateId",
            to = "super::template::Column::Id"
        )]
        Template,
    }

    impl Related<super::template::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Template.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod usage {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "v2_gift_card_usage")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub code_id: i32,
        pub template_id: i32,
        pub user_id: i32,
        pub invite_user_id: Option<i32>,
        pub rewards_given: String,
        pub invite_rewards: Option<String>,
        pub user_level_at_use: Option<i32>,
        pub plan_id_at_use: Option<i32>,
        pub multiplier_applied: f64,
        pub ip_address: Option<String>,
        pub user_agent: Option<String>,
        pub notes: Option<String>,
        pub created_at: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub use code::Entity as GiftCardCode;
pub use template::Entity as GiftCardTemplate;
pub use usage::Entity as GiftCardUsage;
