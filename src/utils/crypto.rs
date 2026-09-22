use crate::common::AppError;
use sha2::{Digest as Sha256Digest, Sha256};

/// Hash a password using Bcrypt with default cost (10).
pub fn hash_password(plain: &str) -> Result<String, AppError> {
    bcrypt::hash(plain, 10)
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {}", e)))
}

/// Verify a plain text password against a hashed string.
/// Compatible with Laravel Bcrypt hashes ($2y$, $2b$, $2a$).
pub fn verify_password(plain: &str, hashed: &str) -> bool {
    // Rust's bcrypt crate natively parses $2a$ and $2b$.
    // If the hash starts with $2y$, normalize it to $2b$ which uses the same algorithm and bugfix.
    if let Some(stripped) = hashed.strip_prefix("$2y$") {
        let normalized = format!("$2b${}", stripped);
        if bcrypt::verify(plain, &normalized).unwrap_or(false) {
            return true;
        }
    }

    bcrypt::verify(plain, hashed).unwrap_or(false)
}

/// Computes CRC32b hash matching PHP `hash('crc32b', $data)`.
/// Returns an 8-character lowercase hexadecimal string.
pub fn crc32b(data: &[u8]) -> String {
    let checksum = crc32fast::hash(data);
    format!("{:08x}", checksum)
}

/// Computes MD5 hash.
/// Returns a 32-character lowercase hexadecimal string.
pub fn md5_hex(data: &[u8]) -> String {
    let digest = md5::compute(data);
    format!("{:x}", digest)
}

/// Computes SHA-1 hash matching PHP `sha1($data)`.
/// Returns a 40-character lowercase hexadecimal string.
pub fn sha1_hex(data: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Computes SHA-256 hash.
/// Returns a 64-character lowercase hexadecimal string.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32b_matching_php() {
        // Known test vector: "hello" -> "3610a686"
        assert_eq!(crc32b(b"hello"), "3610a686");
        // Known test vector: "base64:xboard_default_key_32bytes!!"
        let key_crc = crc32b(b"base64:xboard_default_key_32bytes!!");
        assert_eq!(key_crc.len(), 8);
    }

    #[test]
    fn test_md5_and_sha256_and_sha1() {
        assert_eq!(md5_hex(b"hello"), "5d41402abc4b2a76b9719d911017c592");
        assert_eq!(
            sha1_hex(b"hello"),
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"
        );
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_bcrypt_hashing_and_verification() {
        let password = "my_secret_password_123";
        let hash = hash_password(password).expect("Hashing failed");
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn test_laravel_2y_bcrypt_compatibility() {
        // Pre-computed hash from PHP: password_hash("123456", PASSWORD_BCRYPT)
        // $2y$10$92IXUNpkjO0rOQ5byMi.Ye4oKoEa3Ro9llC/.og/at2.uheWG/igi is Laravel's default factory hash for "password"
        let laravel_hash = "$2y$10$92IXUNpkjO0rOQ5byMi.Ye4oKoEa3Ro9llC/.og/at2.uheWG/igi";
        assert!(verify_password("password", laravel_hash));
        assert!(!verify_password("wrong", laravel_hash));
    }
}
