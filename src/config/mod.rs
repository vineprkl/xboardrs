use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server_host: String,
    pub server_port: u16,
    pub database_url: String,
    pub redis_url: Option<String>,
    pub app_key: String,
    pub app_name: String,
    pub app_url: String,
    pub debug: bool,
}

impl AppConfig {
    pub fn from_env() -> Self {
        // Load .env if exists, ignore error if missing
        let _ = dotenvy::dotenv();
        let _ = dotenvy::from_filename("../.env");

        let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let server_port = env::var("SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(7001);

        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            let db_connection = env::var("DB_CONNECTION").unwrap_or_else(|_| "sqlite".to_string());
            if db_connection == "mysql" {
                let host = env::var("DB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
                let port = env::var("DB_PORT").unwrap_or_else(|_| "3306".to_string());
                let database = env::var("DB_DATABASE").unwrap_or_else(|_| "xboard".to_string());
                let username = env::var("DB_USERNAME").unwrap_or_else(|_| "root".to_string());
                let password = env::var("DB_PASSWORD").unwrap_or_default();
                format!("mysql://{username}:{password}@{host}:{port}/{database}")
            } else {
                let db_file = env::var("DB_DATABASE").unwrap_or_else(|_| "xboard.db".to_string());
                format!("sqlite://{db_file}?mode=rwc")
            }
        });

        let redis_url = env::var("REDIS_URL").ok().or_else(|| {
            let host = env::var("REDIS_HOST").ok()?;
            let port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
            let password = env::var("REDIS_PASSWORD").unwrap_or_default();
            if password.is_empty() {
                Some(format!("redis://{host}:{port}"))
            } else {
                Some(format!("redis://:{password}@{host}:{port}"))
            }
        });

        let app_key = env::var("APP_KEY")
            .unwrap_or_else(|_| "base64:xboard_default_key_32bytes!!".to_string());
        let app_name = env::var("APP_NAME").unwrap_or_else(|_| "Xboard".to_string());
        let app_url = env::var("APP_URL").unwrap_or_else(|_| "http://127.0.0.1:7001".to_string());
        let debug = env::var("APP_DEBUG")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        Self {
            server_host,
            server_port,
            database_url,
            redis_url,
            app_key,
            app_name,
            app_url,
            debug,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::from_env();
        assert_eq!(config.server_host, "0.0.0.0");
        assert_eq!(config.server_port, 7001);
        assert!(!config.database_url.is_empty());
        assert_eq!(config.app_name, "Xboard");
    }
}
