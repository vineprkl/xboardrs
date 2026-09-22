pub mod overall {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "v2_stat")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub record_at: i64,
        pub record_type: String,
        pub order_count: i32,
        pub order_total: i32,
        pub commission_count: i32,
        pub commission_total: i32,
        pub paid_count: i32,
        pub paid_total: i32,
        pub register_count: i32,
        pub invite_count: i32,
        pub transfer_used_total: String,
        pub created_at: i64,
        pub updated_at: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod server {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "v2_stat_server")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub server_id: i32,
        pub server_type: String,
        pub u: i64,
        pub d: i64,
        pub record_type: String,
        pub record_at: i64,
        pub created_at: i64,
        pub updated_at: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod user {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "v2_stat_user")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub user_id: i32,
        pub server_rate: f64,
        pub u: i64,
        pub d: i64,
        pub record_type: String,
        pub record_at: i64,
        pub created_at: i64,
        pub updated_at: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "crate::entities::user::Entity",
            from = "Column::UserId",
            to = "crate::entities::user::Column::Id"
        )]
        User,
    }

    impl Related<crate::entities::user::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub use overall::Entity as Stat;
pub use server::Entity as StatServer;
pub use user::Entity as StatUser;
