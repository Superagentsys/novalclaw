//! Password security module for account management.
//!
//! Uses Argon2id algorithm for secure password hashing with the following properties:
//! - Resistant to GPU/ASIC attacks
//! - Memory-hard algorithm
//! - Random salt for each hash
//!
//! [Source: 2-11-local-account-management.md]

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

/// Password-related errors
#[derive(Debug, Error)]
pub enum PasswordError {
    /// Password hashing failed
    #[error("密码哈希失败: {0}")]
    HashFailed(String),

    /// Password verification failed
    #[error("密码验证失败: {0}")]
    VerifyFailed(String),

    /// Password too short
    #[error("密码长度至少8个字符")]
    TooShort,

    /// Password too long
    #[error("密码长度不能超过128个字符")]
    TooLong,
}

/// Hash a password using Argon2id.
///
/// # Arguments
/// * `password` - The plain text password to hash
///
/// # Returns
/// The hashed password string (PHC format)
///
/// # Example
/// ```rust
/// use omninova_core::security::password::hash_password;
///
/// let hash = hash_password("my_secure_password")?;
/// assert!(hash.starts_with("$argon2id$"));
/// # Ok::<(), omninova_core::security::password::PasswordError>(())
/// ```
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| PasswordError::HashFailed(e.to_string()))
}

/// Verify a password against a hash.
///
/// # Arguments
/// * `password` - The plain text password to verify
/// * `hash` - The stored password hash (PHC format)
///
/// # Returns
/// `true` if the password matches, `false` otherwise
///
/// # Example
/// ```rust
/// use omninova_core::security::password::{hash_password, verify_password};
///
/// let hash = hash_password("my_secure_password")?;
/// assert!(verify_password("my_secure_password", &hash)?);
/// assert!(!verify_password("wrong_password", &hash)?);
/// # Ok::<(), omninova_core::security::password::PasswordError>(())
/// ```
pub fn verify_password(password: &str, hash: &str) -> Result<bool, PasswordError> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| PasswordError::VerifyFailed(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Validate password strength.
///
/// Current requirements:
/// - Minimum 8 characters
/// - Maximum 128 characters
///
/// # Arguments
/// * `password` - The password to validate
///
/// # Errors
/// Returns error if password doesn't meet requirements
///
/// # Example
/// ```rust
/// use omninova_core::security::password::validate_password_strength;
///
/// assert!(validate_password_strength("short").is_err()); // Error: too short
/// assert!(validate_password_strength("long_enough_password").is_ok()); // OK
/// ```
pub fn validate_password_strength(password: &str) -> Result<(), PasswordError> {
    if password.len() < 8 {
        return Err(PasswordError::TooShort);
    }
    if password.len() > 128 {
        return Err(PasswordError::TooLong);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password_creates_valid_hash() {
        let password = "test_password_123";
        let hash = hash_password(password).expect("Hash should succeed");

        // Argon2id hashes start with $argon2id$
        assert!(hash.starts_with("$argon2id$"));
        assert!(!hash.contains(password));
    }

    #[test]
    fn test_hash_password_creates_unique_salts() {
        let password = "same_password";
        let hash1 = hash_password(password).expect("Hash should succeed");
        let hash2 = hash_password(password).expect("Hash should succeed");

        // Same password should produce different hashes due to random salt
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_verify_password_correct() {
        let password = "correct_password";
        let hash = hash_password(password).expect("Hash should succeed");

        let result = verify_password(password, &hash).expect("Verify should succeed");
        assert!(result);
    }

    #[test]
    fn test_verify_password_incorrect() {
        let password = "correct_password";
        let hash = hash_password(password).expect("Hash should succeed");

        let result = verify_password("wrong_password", &hash).expect("Verify should succeed");
        assert!(!result);
    }

    #[test]
    fn test_validate_password_strength_valid() {
        assert!(validate_password_strength("8charact").is_ok());
        assert!(validate_password_strength("normal_password").is_ok());
        assert!(validate_password_strength("a".repeat(128).as_str()).is_ok());
    }

    #[test]
    fn test_validate_password_strength_too_short() {
        assert!(validate_password_strength("7chars!").is_err());
        assert!(validate_password_strength("").is_err());
        assert!(validate_password_strength("short").is_err());
    }

    #[test]
    fn test_validate_password_strength_too_long() {
        let long_password = "a".repeat(129);
        assert!(validate_password_strength(&long_password).is_err());
    }
}