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
    if config.database_url.starts_with("sqlite:") {
        ensure_sqlite_parent_dir(&config.database_url).await?;
    }
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

    // Handle CLI subcommands for administrator & user management
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "admin" | "admin:create" => {
                if args.len() < 4 {
                    eprintln!("Error: Missing arguments for admin command.");
                    eprintln!("Usage: {} admin <email> <password>", args[0]);
                    std::process::exit(1);
                }
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
            "reset:password" => {
                if args.len() < 4 {
                    eprintln!("Error: Missing arguments for reset:password command.");
                    eprintln!("Usage: {} reset:password <email> <new_password>", args[0]);
                    std::process::exit(1);
                }
                let email = &args[2];
                let password = &args[3];
                match user_service.reset_user_password(&db, email, password).await {
                    Ok(user) => {
                        println!("============================================================");
                        println!("  User password has been reset successfully!");
                        println!("  Email: {}", user.email);
                        println!(
                            "  Is Administrator: {}",
                            if user.is_admin { "Yes" } else { "No" }
                        );
                        println!("  (Notice: Existing login sessions have been invalidated)");
                        println!("============================================================");
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("Error: Failed to reset password: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            "help" | "--help" | "-h" => {
                println!("⚡ Xboard-RS CLI Management Tool");
                println!("\nUsage:");
                println!("  {} [command] [options]\n", args[0]);
                println!("Commands:");
                println!(
                    "  admin <email> <password>           Create or update administrator account"
                );
                println!("  reset:password <email> <password>  Reset user password without changing permissions");
                println!("  help, --help, -h                   Show this help message\n");
                println!("Run without any arguments to start the HTTP web server.");
                return Ok(());
            }
            unknown => {
                eprintln!("Error: Unknown command '{}'.", unknown);
                eprintln!("Run '{} --help' to view available commands.", args[0]);
                std::process::exit(1);
            }
        }
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

/// Recursively ensures that the parent directory of an SQLite database file exists before connecting.
async fn ensure_sqlite_parent_dir(database_url: &str) -> anyhow::Result<()> {
    let raw_path = database_url
        .trim_start_matches("sqlite://")
        .trim_start_matches("sqlite:");
    let path_str = raw_path.split('?').next().unwrap_or(raw_path);
    if path_str == ":memory:" || path_str.is_empty() {
        return Ok(());
    }

    let p = std::path::Path::new(path_str);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            tracing::info!(
                "SQLite parent directory {:?} does not exist, creating recursively...",
                parent
            );
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to create SQLite database parent directory {:?}: {}. Please check filesystem permissions.",
                    parent,
                    e
                )
            })?;
        }
    }
    Ok(())
}
