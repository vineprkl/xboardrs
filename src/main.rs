use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use xboard_rs::{
    app_router_with_state,
    common::AppState,
    config::AppConfig,
    cron::start_cron_scheduler,
    services::{
        AuthService, DeviceStateService, PlanService, ServerService, SettingService, UserService,
    },
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xboard_rs=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. Load configuration from environment / .env
    let config = AppConfig::from_env();
    let addr: SocketAddr = format!("{}:{}", config.server_host, config.server_port).parse()?;

    tracing::info!("Initializing {} database connection...", config.app_name);

    // 3. Connect to database (MySQL or SQLite)
    let db = sea_orm::Database::connect(&config.database_url).await?;

    // Auto-migrate tables if not existing
    if let Err(e) = xboard_rs::entities::auto_migrate(&db).await {
        tracing::warn!("Warning: auto_migrate error: {:?}", e);
    }

    // 4. Initialize services and application state
    let setting_service = SettingService::new(db.clone());
    if let Err(e) = setting_service.load_all().await {
        tracing::warn!("Warning: Failed to pre-load settings: {:?}", e);
    }

    let device_state_service = DeviceStateService::new();
    let server_service = ServerService::new(
        db.clone(),
        setting_service.clone(),
        device_state_service.clone(),
    );
    let plan_service = PlanService::new(db.clone());
    let user_service = UserService::new();
    let auth_service = AuthService::new(db.clone(), setting_service.clone());

    // Handle CLI subcommands for administrator management
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4
        && (args[1] == "admin" || args[1] == "admin:create" || args[1] == "reset:password")
    {
        let email = &args[2];
        let password = &args[3];
        user_service
            .create_or_reset_admin(&db, email, password)
            .await?;
        let secure_path = setting_service.get_string("secure_path", "").await;
        let admin_path = if !secure_path.is_empty() {
            secure_path
        } else {
            let app_key = std::env::var("APP_KEY")
                .unwrap_or_else(|_| "base64:xboard_default_key_32bytes!!".to_string());
            xboard_rs::utils::crc32b(app_key.as_bytes())
        };
        println!("============================================================");
        println!("  Administrator account saved successfully!");
        println!("  Email: {}", email);
        println!("  Password: {}", password);
        println!(
            "  Admin panel URL: http://{}:{}/{}",
            config.server_host, config.server_port, admin_path
        );
        println!("============================================================");
        return Ok(());
    }

    // Auto-seed default administrator account if none exists
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
    let admin_count = xboard_rs::entities::User::find()
        .filter(xboard_rs::entities::user::Column::IsAdmin.eq(true))
        .count(&db)
        .await?;

    if admin_count == 0 {
        let default_email =
            std::env::var("ADMIN_EMAIL").unwrap_or_else(|_| "admin@demo.com".to_string());
        let default_password =
            std::env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin123456".to_string());
        user_service
            .create_or_reset_admin(&db, &default_email, &default_password)
            .await?;
        let secure_path = setting_service.get_string("secure_path", "").await;
        let admin_path = if !secure_path.is_empty() {
            secure_path
        } else {
            let app_key = std::env::var("APP_KEY")
                .unwrap_or_else(|_| "base64:xboard_default_key_32bytes!!".to_string());
            xboard_rs::utils::crc32b(app_key.as_bytes())
        };
        tracing::info!("============================================================");
        tracing::info!("  Initialized default administrator account:");
        tracing::info!("  Email: {}", default_email);
        tracing::info!("  Password: {}", default_password);
        tracing::info!(
            "  Admin panel URL: http://{}:{}/{}",
            config.server_host,
            config.server_port,
            admin_path
        );
        tracing::info!("============================================================");
    }

    let state = AppState::new(
        db,
        setting_service,
        device_state_service,
        server_service,
        plan_service,
        user_service,
        auth_service,
    );

    // 5. Start background cron worker loop
    tracing::info!("Starting background cron scheduler...");
    let _cron_handle = start_cron_scheduler(state.clone());

    // 6. Build HTTP router with full SPA & API support
    let app = app_router_with_state(state);

    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
