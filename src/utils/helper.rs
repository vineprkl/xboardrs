use chrono::Utc;
use rand::{distributions::Alphanumeric, Rng};
use uuid::Uuid;

/// Generates an order trade number matching Laravel `Helper::generateOrderNo()`.
/// Format: YmdHis + microsecond (6 digits) + random 5 digits.
pub fn generate_order_no() -> String {
    let now = Utc::now();
    let date_str = now.format("%Y%m%d%H%M%S").to_string();
    let micros = now.timestamp_subsec_micros();
    let mut rng = rand::thread_rng();
    let random_num: u32 = rng.gen_range(10000..=99999);

    format!("{}{:06}{}", date_str, micros, random_num)
}

/// Generates a standard UUID v4 string.
pub fn generate_uuid() -> String {
    Uuid::new_v4().to_string()
}

/// Generates a random alphanumeric token with specified length.
pub fn random_char(len: usize, special: bool) -> String {
    let mut rng = rand::thread_rng();
    if special {
        const CHARS: &[u8] =
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$?|{}";
        (0..len)
            .map(|_| {
                let idx = rng.gen_range(0..CHARS.len());
                CHARS[idx] as char
            })
            .collect()
    } else {
        (0..len).map(|_| rng.sample(Alphanumeric) as char).collect()
    }
}

/// Formats bytes into a human readable traffic string (e.g. "10.50 GB").
pub fn traffic_format(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Generates a server key for Shadowsocks 2022 matching PHP `Helper::getServerKey()`.
pub fn get_server_key(timestamp: i64, length: usize) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let md5_str = crate::utils::md5_hex(timestamp.to_string().as_bytes());
    let len = length.min(md5_str.len());
    STANDARD.encode(&md5_str.as_bytes()[..len])
}

/// Converts UUID prefix to Base64 matching PHP `Helper::uuidToBase64()`.
pub fn uuid_to_base64(uuid: &str, length: usize) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let len = length.min(uuid.len());
    STANDARD.encode(&uuid.as_bytes()[..len])
}

/// Resolves dynamic port range (e.g. "10000-20000") to a single port matching PHP `Helper::randomPort()`.
pub fn resolve_port(port_str: &str) -> String {
    if let Some((min_str, max_str)) = port_str.split_once('-') {
        if let (Ok(min), Ok(max)) = (min_str.trim().parse::<u16>(), max_str.trim().parse::<u16>()) {
            if min <= max {
                let mut rng = rand::thread_rng();
                let picked = rng.gen_range(min..=max);
                return picked.to_string();
            }
        }
    }
    port_str.to_string()
}

/// Constructs a full subscription URL matching PHP Helper::getSubscribeUrl().
pub fn get_subscribe_url(app_url: &str, subscribe_path: &str, token: &str) -> String {
    let base = if app_url.trim().is_empty() {
        "".to_string()
    } else {
        app_url.trim_end_matches('/').to_string()
    };
    let path = subscribe_path.trim_matches('/');
    let token = token.trim();
    if base.is_empty() {
        format!("/{}/{}", path, token)
    } else {
        format!("{}/{}/{}", base, path, token)
    }
}

/// Resolves the application base directory with fallback priority:
/// 1. `APP_BASE_DIR` environment variable
/// 2. Directory of the running executable (`std::env::current_exe()?.parent()`)
/// 3. Current working directory (`std::env::current_dir()`)
pub fn get_app_base_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("APP_BASE_DIR") {
        if !dir.trim().is_empty() {
            return std::path::PathBuf::from(dir);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            // In development, the executable is placed in target/debug or target/release.
            // If parent doesn't have public or config.yaml, check the workspace root.
            if !parent.join("public").exists() && !parent.join("config.yaml").exists() {
                if let Some(workspace_root) = parent.parent().and_then(|p| p.parent()) {
                    if workspace_root.join("public").exists()
                        || workspace_root.join("config.yaml").exists()
                    {
                        return workspace_root.to_path_buf();
                    }
                }
            }
            return parent.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_no_format_and_uniqueness() {
        let no1 = generate_order_no();
        let no2 = generate_order_no();

        assert_ne!(no1, no2);
        // Length: YmdHis (14) + micros (6) + rand (5) = 25 chars
        assert_eq!(no1.len(), 25);
        assert!(no1.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_uuid_v4_generation() {
        let u1 = generate_uuid();
        assert_eq!(u1.len(), 36);
        assert_eq!(u1.chars().filter(|&c| c == '-').count(), 4);
    }

    #[test]
    fn test_random_char() {
        let token = random_char(32, false);
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_traffic_format() {
        assert_eq!(traffic_format(500), "500 B");
        assert_eq!(traffic_format(1024), "1.00 KB");
        assert_eq!(traffic_format(1024 * 1024 * 50), "50.00 MB");
        assert_eq!(traffic_format(1024 * 1024 * 1024 * 2), "2.00 GB");
        assert_eq!(traffic_format(2 * 1024 * 1024 * 1024 * 1024), "2.00 TB");
    }

    #[test]
    fn test_server_key_and_uuid_to_base64() {
        let key16 = get_server_key(1700000000, 16);
        assert!(!key16.is_empty());
        let u_b64 = uuid_to_base64("a5933994-01be-4977-bc6d-d1efdf76856a", 16);
        assert!(!u_b64.is_empty());
    }
}
