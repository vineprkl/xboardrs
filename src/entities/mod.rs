pub mod admin_audit_log;
pub mod commission_log;
pub mod coupon;
pub mod gift_card;
pub mod invite_code;
pub mod knowledge;
pub mod notice;
pub mod order;
pub mod payment;
pub mod personal_access_token;
pub mod plan;
pub mod server;
pub mod server_group;
pub mod server_machine;
pub mod server_machine_load_history;
pub mod server_route;
pub mod setting;
pub mod stat;
pub mod ticket;
pub mod ticket_message;
pub mod traffic_reset_log;
pub mod user;

pub use admin_audit_log::Entity as AdminAuditLog;
pub use commission_log::Entity as CommissionLog;
pub use coupon::Entity as Coupon;
pub use gift_card::{GiftCardCode, GiftCardTemplate, GiftCardUsage};
pub use invite_code::Entity as InviteCode;
pub use knowledge::Entity as Knowledge;
pub use notice::Entity as Notice;
pub use order::Entity as Order;
pub use payment::Entity as Payment;
pub use personal_access_token::Entity as PersonalAccessToken;
pub use plan::Entity as Plan;
pub use server::Entity as Server;
pub use server_group::Entity as ServerGroup;
pub use server_machine::Entity as ServerMachine;
pub use server_machine_load_history::Entity as ServerMachineLoadHistory;
pub use server_route::Entity as ServerRoute;
pub use setting::Entity as Setting;
pub use stat::{Stat, StatServer, StatUser};
pub use ticket::Entity as Ticket;
pub use ticket_message::Entity as TicketMessage;
pub use traffic_reset_log::Entity as TrafficResetLog;
pub use user::Entity as User;

use crate::common::AppError;
use sea_orm::{ConnectionTrait, DatabaseConnection, Schema};

/// Automatically creates all entity tables if they do not exist.
/// Safe to run against existing databases (using IF NOT EXISTS).
pub async fn auto_migrate(db: &DatabaseConnection) -> Result<(), AppError> {
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);

    let tables = vec![
        schema.create_table_from_entity(ServerGroup),
        schema.create_table_from_entity(Plan),
        schema.create_table_from_entity(Server),
        schema.create_table_from_entity(ServerMachine),
        schema.create_table_from_entity(ServerMachineLoadHistory),
        schema.create_table_from_entity(ServerRoute),
        schema.create_table_from_entity(User),
        schema.create_table_from_entity(Setting),
        schema.create_table_from_entity(InviteCode),
        schema.create_table_from_entity(PersonalAccessToken),
        schema.create_table_from_entity(Notice),
        schema.create_table_from_entity(Knowledge),
        schema.create_table_from_entity(Coupon),
        schema.create_table_from_entity(Ticket),
        schema.create_table_from_entity(TicketMessage),
        schema.create_table_from_entity(GiftCardTemplate),
        schema.create_table_from_entity(GiftCardCode),
        schema.create_table_from_entity(GiftCardUsage),
        schema.create_table_from_entity(CommissionLog),
        schema.create_table_from_entity(TrafficResetLog),
        schema.create_table_from_entity(Order),
        schema.create_table_from_entity(Payment),
        schema.create_table_from_entity(Stat),
        schema.create_table_from_entity(StatUser),
        schema.create_table_from_entity(StatServer),
        schema.create_table_from_entity(AdminAuditLog),
    ];

    for mut stmt in tables {
        stmt.if_not_exists();
        let _ = db.execute(backend.build(&stmt)).await;
    }

    Ok(())
}
