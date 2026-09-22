use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "v2_order")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub invite_user_id: Option<i32>,
    pub user_id: i32,
    pub plan_id: i32,
    pub coupon_id: Option<i32>,
    pub payment_id: Option<i32>,
    pub r#type: i32,
    pub period: String,
    pub trade_no: String,
    pub callback_no: Option<String>,
    pub total_amount: i32,
    pub handling_amount: Option<i32>,
    pub discount_amount: Option<i32>,
    pub surplus_amount: Option<i32>,
    pub refund_amount: Option<i32>,
    pub balance_amount: Option<i32>,
    pub surplus_credit: Option<i32>,
    pub surplus_order_ids: Option<String>,
    pub status: i32,
    pub commission_status: i32,
    pub commission_balance: i32,
    pub actual_commission_balance: Option<i32>,
    pub commission_rate: Option<f64>,
    pub commission_auto_check: Option<bool>,
    pub paid_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::plan::Entity",
        from = "Column::PlanId",
        to = "super::plan::Column::Id"
    )]
    Plan,
    #[sea_orm(
        belongs_to = "super::coupon::Entity",
        from = "Column::CouponId",
        to = "super::coupon::Column::Id"
    )]
    Coupon,
    #[sea_orm(
        belongs_to = "super::payment::Entity",
        from = "Column::PaymentId",
        to = "super::payment::Column::Id"
    )]
    Payment,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::plan::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Plan.def()
    }
}

impl Related<super::coupon::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Coupon.def()
    }
}

impl Related<super::payment::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Payment.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
