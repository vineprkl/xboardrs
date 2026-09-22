use crate::services::{
    order_service::OrderService, AuthService, CouponService, DeviceStateService, GiftCardService,
    PlanService, ServerService, SettingService, TicketService, UserService,
};
use sea_orm::DatabaseConnection;

/// Shared application state accessible across all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub setting_service: SettingService,
    pub device_state_service: DeviceStateService,
    pub server_service: ServerService,
    pub plan_service: PlanService,
    pub user_service: UserService,
    pub auth_service: AuthService,
    pub coupon_service: CouponService,
    pub ticket_service: TicketService,
    pub gift_card_service: GiftCardService,
    pub order_service: OrderService,
}

impl AppState {
    pub fn new(
        db: DatabaseConnection,
        setting_service: SettingService,
        device_state_service: DeviceStateService,
        server_service: ServerService,
        plan_service: PlanService,
        user_service: UserService,
        auth_service: AuthService,
    ) -> Self {
        let coupon_service = CouponService::new(db.clone());
        let ticket_service = TicketService::new(db.clone());
        let gift_card_service = GiftCardService::new(db.clone());
        let order_service = OrderService::new(
            db.clone(),
            plan_service.clone(),
            coupon_service.clone(),
            setting_service.clone(),
        );
        Self {
            db,
            setting_service,
            device_state_service,
            server_service,
            plan_service,
            user_service,
            auth_service,
            coupon_service,
            ticket_service,
            gift_card_service,
            order_service,
        }
    }
}
