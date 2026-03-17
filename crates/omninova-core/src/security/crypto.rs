//! Data encryption module for sensitive data protection.
//!
//! Provides AES-256-GCM encryption for local data storage with:
//! - Hardware-accelerated encryption when available
//! - Secure key derivation using Argon2id
//! - OS Keychain integration for key storage
//!
//! [Source: 2-13-data-encryption-privacy.md]

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{
    password_hash::SaltString,
    Argon2,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

// ============================================================================
// Constants
// ============================================================================

/// Encryption format version
const ENCRYPTION_VERSION: u8 = 1;

/// Nonce size for AES-GCM (12 bytes)
const NONCE_SIZE: usize = 12;

/// Key size for AES-256 (32 bytes)
const KEY_SIZE: usize = 32;

/// Salt size for key derivation
const SALT_SIZE: usize = 16;

/// Prefix for encrypted data
const ENCRYPTED_PREFIX: &[u8] = b"ENCv1:";

// ============================================================================
// Error Types
// ============================================================================

/// Encryption-related errors
#[derive(Debug, Error)]
pub enum EncryptionError {
    /// Encryption failed
    #[error("加密失败: {0}")]
    EncryptionFailed(String),

    /// Decryption failed
    #[error("解密失败: {0}")]
    DecryptionFailed(String),

    /// Key derivation failed
    #[error("密钥派生失败: {0}")]
    KeyDerivationFailed(String),

    /// Invalid key
    #[error("无效的加密密钥")]
    InvalidKey,

    /// Data is not encrypted
    #[error("数据未加密")]
    NotEncrypted,

    /// Invalid encrypted data format
    #[error("无效的加密数据格式")]
    InvalidFormat,

    /// Keychain access failed
    #[error("Keychain 访问失败: {0}")]
    KeychainAccessFailed(String),

    /// Encryption not available
    #[error("加密功能不可用")]
    NotAvailable,
}

// ============================================================================
// Data Types
// ============================================================================

/// Encryption key information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptionKeyInfo {
    /// Key ID (used to identify the key in keychain)
    pub key_id: String,
    /// Salt used for key derivation (base64 encoded)
    pub salt: String,
    /// Key creation timestamp
    pub created_at: i64,
}

/// Encryption settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptionSettings {
    /// Whether encryption is enabled
    pub enabled: bool,
    /// Key information (if encryption is enabled)
    pub key_info: Option<EncryptionKeyInfo>,
    /// Last updated timestamp
    pub updated_at: i64,
}

impl Default for EncryptionSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            key_info: None,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

// ============================================================================
// Encryption Service Trait
// ============================================================================

/// Trait for encryption services
pub trait EncryptionService: Send + Sync {
    /// Encrypt data
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError>;

    /// Decrypt data
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError>;

    /// Check if data is encrypted
    fn is_encrypted(&self, data: &[u8]) -> bool;

    /// Check if encryption is available
    fn is_available(&self) -> bool;

    /// Get encryption settings
    fn settings(&self) -> EncryptionSettings;
}

// ============================================================================
// AES-256-GCM Implementation
// ============================================================================

/// AES-256-GCM encryption service
pub struct AesGcmEncryption {
    /// Encryption key (32 bytes for AES-256)
    key: [u8; KEY_SIZE],
    /// Encryption settings
    settings: EncryptionSettings,
}

impl AesGcmEncryption {
    /// Create a new encryption service with a derived key
    ///
    /// # Arguments
    /// * `password` - Password to derive the key from
    /// * `salt` - Salt for key derivation (optional, generates if None)
    ///
    /// # Returns
    /// A new encryption service instance
    pub fn new(password: &str, salt: Option<&[u8]>) -> Result<Self, EncryptionError> {
        let salt_bytes = match salt {
            Some(s) => {
                if s.len() < SALT_SIZE {
                    return Err(EncryptionError::KeyDerivationFailed(
                        "Salt too short".to_string(),
                    ));
                }
                let mut arr = [0u8; SALT_SIZE];
                arr.copy_from_slice(&s[..SALT_SIZE]);
                arr
            }
            None => {
                let mut arr = [0u8; SALT_SIZE];
                rand::thread_rng().fill_bytes(&mut arr);
                arr
            }
        };

        let key = Self::derive_key(password, &salt_bytes)?;

        let key_info = EncryptionKeyInfo {
            key_id: uuid::Uuid::new_v4().to_string(),
            salt: BASE64.encode(salt_bytes),
            created_at: chrono::Utc::now().timestamp(),
        };

        Ok(Self {
            key,
            settings: EncryptionSettings {
                enabled: true,
                key_info: Some(key_info),
                updated_at: chrono::Utc::now().timestamp(),
            },
        })
    }

    /// Create from an existing key
    pub fn from_key(key: [u8; KEY_SIZE]) -> Self {
        let key_info = EncryptionKeyInfo {
            key_id: uuid::Uuid::new_v4().to_string(),
            salt: String::new(),
            created_at: chrono::Utc::now().timestamp(),
        };

        Self {
            key,
            settings: EncryptionSettings {
                enabled: true,
                key_info: Some(key_info),
                updated_at: chrono::Utc::now().timestamp(),
            },
        }
    }

    /// Derive encryption key from password using Argon2id
    fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_SIZE], EncryptionError> {
        let salt_string = SaltString::encode_b64(salt)
            .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;

        let argon2 = Argon2::default();
        let mut key = [0u8; KEY_SIZE];

        argon2
            .hash_password_into(password.as_bytes(), salt_string.as_str().as_bytes(), &mut key)
            .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;

        Ok(key)
    }

    /// Generate a random nonce
    fn generate_nonce() -> [u8; NONCE_SIZE] {
        let mut nonce = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce);
        nonce
    }
}

impl EncryptionService for AesGcmEncryption {
    /// Encrypt data using AES-256-GCM
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let cipher =
            Aes256Gcm::new_from_slice(&self.key).map_err(|e| {
                EncryptionError::EncryptionFailed(format!("Invalid key: {}", e))
            })?;

        let nonce_bytes = Self::generate_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

        // Format: ENCRYPTED_PREFIX || version || nonce || ciphertext
        let mut result = Vec::with_capacity(
            ENCRYPTED_PREFIX.len() + 1 + NONCE_SIZE + ciphertext.len(),
        );
        result.extend_from_slice(ENCRYPTED_PREFIX);
        result.push(ENCRYPTION_VERSION);
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt data using AES-256-GCM
    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        // Check prefix
        if !data.starts_with(ENCRYPTED_PREFIX) {
            return Err(EncryptionError::NotEncrypted);
        }

        let data = &data[ENCRYPTED_PREFIX.len()..];

        // Check version
        if data.is_empty() {
            return Err(EncryptionError::InvalidFormat);
        }
        let version = data[0];
        if version != ENCRYPTION_VERSION {
            return Err(EncryptionError::DecryptionFailed(format!(
                "Unsupported version: {}",
                version
            )));
        }
        let data = &data[1..];

        // Extract nonce and ciphertext
        if data.len() < NONCE_SIZE {
            return Err(EncryptionError::InvalidFormat);
        }
        let nonce = Nonce::from_slice(&data[..NONCE_SIZE]);
        let ciphertext = &data[NONCE_SIZE..];

        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| EncryptionError::DecryptionFailed(format!("Invalid key: {}", e)))?;

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))
    }

    /// Check if data is encrypted
    fn is_encrypted(&self, data: &[u8]) -> bool {
        data.starts_with(ENCRYPTED_PREFIX)
    }

    /// Check if encryption is available
    fn is_available(&self) -> bool {
        true
    }

    /// Get encryption settings
    fn settings(&self) -> EncryptionSettings {
        self.settings.clone()
    }
}

// ============================================================================
// Encryption Key Manager
// ============================================================================

/// Manages encryption keys stored in OS Keychain
pub struct EncryptionKeyManager {
    /// Key identifier for keychain storage
    keychain_key: String,
    /// Current encryption service (if enabled)
    service: Arc<RwLock<Option<AesGcmEncryption>>>,
    /// Settings cache
    settings: Arc<RwLock<EncryptionSettings>>,
}

impl EncryptionKeyManager {
    /// Service name for keychain entries
    const KEYCHAIN_SERVICE: &'static str = "com.omninoval.encryption";

    /// Create a new key manager
    pub fn new() -> Self {
        Self {
            keychain_key: format!("{}_master_key", Self::KEYCHAIN_SERVICE),
            service: Arc::new(RwLock::new(None)),
            settings: Arc::new(RwLock::new(EncryptionSettings::default())),
        }
    }

    /// Enable encryption with a password
    pub async fn enable_encryption(&self, password: &str) -> Result<(), EncryptionError> {
        // Generate a random master key
        let mut master_key = [0u8; KEY_SIZE];
        rand::thread_rng().fill_bytes(&mut master_key);

        // Encrypt the master key with the password
        let encryption = AesGcmEncryption::new(password, None)?;
        let encrypted_master_key = encryption.encrypt(&master_key)?;

        // Store the encrypted master key
        self.store_key_in_keychain(&encrypted_master_key).await?;

        // Create encryption service with master key
        let service = AesGcmEncryption::from_key(master_key);
        let settings = service.settings.clone();

        *self.service.write().await = Some(service);
        *self.settings.write().await = settings;

        Ok(())
    }

    /// Disable encryption
    pub async fn disable_encryption(&self) -> Result<(), EncryptionError> {
        // Remove key from keychain
        self.remove_key_from_keychain().await?;

        // Clear encryption service
        *self.service.write().await = None;
        *self.settings.write().await = EncryptionSettings::default();

        Ok(())
    }

    /// Unlock encryption with password
    pub async fn unlock(&self, password: &str) -> Result<(), EncryptionError> {
        // Get encrypted master key from keychain
        let encrypted_master_key = self.get_key_from_keychain().await?;

        // Decrypt master key
        let encryption = AesGcmEncryption::new(password, None)?;
        let master_key_bytes = encryption.decrypt(&encrypted_master_key)?;

        if master_key_bytes.len() != KEY_SIZE {
            return Err(EncryptionError::InvalidKey);
        }

        let mut master_key = [0u8; KEY_SIZE];
        master_key.copy_from_slice(&master_key_bytes);

        // Create encryption service with master key
        let service = AesGcmEncryption::from_key(master_key);
        let settings = service.settings.clone();

        *self.service.write().await = Some(service);
        *self.settings.write().await = settings;

        Ok(())
    }

    /// Get encryption settings
    pub async fn settings(&self) -> EncryptionSettings {
        self.settings.read().await.clone()
    }

    /// Check if encryption is enabled
    pub async fn is_enabled(&self) -> bool {
        self.settings.read().await.enabled
    }

    /// Encrypt data
    pub async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let service = self.service.read().await;
        match service.as_ref() {
            Some(s) => s.encrypt(plaintext),
            None => Err(EncryptionError::NotAvailable),
        }
    }

    /// Decrypt data
    pub async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let service = self.service.read().await;
        match service.as_ref() {
            Some(s) => s.decrypt(ciphertext),
            None => {
                // If encryption is not enabled, return plaintext as-is
                if !self.is_encrypted(ciphertext).await {
                    Ok(ciphertext.to_vec())
                } else {
                    Err(EncryptionError::NotAvailable)
                }
            }
        }
    }

    /// Check if data is encrypted
    pub async fn is_encrypted(&self, data: &[u8]) -> bool {
        data.starts_with(ENCRYPTED_PREFIX)
    }

    // Keychain operations (placeholder - actual implementation depends on platform)

    async fn store_key_in_keychain(&self, key: &[u8]) -> Result<(), EncryptionError> {
        // TODO: Implement actual keychain storage
        // For now, store in a secure file (this is a placeholder)
        let key_b64 = BASE64.encode(key);
        let config_dir = directories::ProjectDirs::from("com", "omninoval", "omninoval")
            .ok_or_else(|| EncryptionError::KeychainAccessFailed("Cannot find config directory".to_string()))?;
        let data_dir = config_dir.data_dir();
        let key_file = data_dir.join(".encryption_key");

        // Create directory if it doesn't exist
        tokio::fs::create_dir_all(data_dir)
            .await
            .map_err(|e| EncryptionError::KeychainAccessFailed(e.to_string()))?;

        tokio::fs::write(&key_file, key_b64)
            .await
            .map_err(|e| EncryptionError::KeychainAccessFailed(e.to_string()))?;

        Ok(())
    }

    async fn get_key_from_keychain(&self) -> Result<Vec<u8>, EncryptionError> {
        // TODO: Implement actual keychain retrieval
        let config_dir = directories::ProjectDirs::from("com", "omninoval", "omninoval")
            .ok_or_else(|| EncryptionError::KeychainAccessFailed("Cannot find config directory".to_string()))?;
        let key_file = config_dir.data_dir().join(".encryption_key");

        if !key_file.exists() {
            return Err(EncryptionError::KeychainAccessFailed("Key not found".to_string()));
        }

        let key_b64 = tokio::fs::read_to_string(&key_file)
            .await
            .map_err(|e| EncryptionError::KeychainAccessFailed(e.to_string()))?;

        BASE64
            .decode(key_b64.trim())
            .map_err(|e| EncryptionError::KeychainAccessFailed(e.to_string()))
    }

    async fn remove_key_from_keychain(&self) -> Result<(), EncryptionError> {
        // TODO: Implement actual keychain removal
        let config_dir = directories::ProjectDirs::from("com", "omninoval", "omninoval")
            .ok_or_else(|| EncryptionError::KeychainAccessFailed("Cannot find config directory".to_string()))?;
        let key_file = config_dir.data_dir().join(".encryption_key");

        if key_file.exists() {
            tokio::fs::remove_file(&key_file)
                .await
                .map_err(|e| EncryptionError::KeychainAccessFailed(e.to_string()))?;
        }

        Ok(())
    }
}

impl Default for EncryptionKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Encrypt a string value
pub fn encrypt_string(plaintext: &str, key: &[u8; KEY_SIZE]) -> Result<String, EncryptionError> {
    let service = AesGcmEncryption::from_key(*key);
    let encrypted = service.encrypt(plaintext.as_bytes())?;
    Ok(BASE64.encode(&encrypted))
}

/// Decrypt a string value
pub fn decrypt_string(ciphertext: &str, key: &[u8; KEY_SIZE]) -> Result<String, EncryptionError> {
    let encrypted = BASE64
        .decode(ciphertext)
        .map_err(|_| EncryptionError::InvalidFormat)?;
    let service = AesGcmEncryption::from_key(*key);
    let decrypted = service.decrypt(&encrypted)?;
    String::from_utf8(decrypted).map_err(|_| EncryptionError::DecryptionFailed("Invalid UTF-8".to_string()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_service() -> AesGcmEncryption {
        let mut key = [0u8; KEY_SIZE];
        rand::thread_rng().fill_bytes(&mut key);
        AesGcmEncryption::from_key(key)
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let service = create_test_service();
        let plaintext = b"Hello, World! This is a secret message.";

        let encrypted = service.encrypt(plaintext).expect("Encryption should succeed");
        let decrypted = service.decrypt(&encrypted).expect("Decryption should succeed");

        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let service = create_test_service();
        let plaintext = b"Same message";

        let encrypted1 = service.encrypt(plaintext).expect("Encryption should succeed");
        let encrypted2 = service.encrypt(plaintext).expect("Encryption should succeed");

        // Same plaintext should produce different ciphertext (due to random nonce)
        assert_ne!(encrypted1, encrypted2);
    }

    #[test]
    fn test_is_encrypted_detects_encrypted_data() {
        let service = create_test_service();
        let plaintext = b"Test message";

        let encrypted = service.encrypt(plaintext).expect("Encryption should succeed");

        assert!(!service.is_encrypted(plaintext));
        assert!(service.is_encrypted(&encrypted));
    }

    #[test]
    fn test_decrypt_unencrypted_data_fails() {
        let service = create_test_service();
        let plaintext = b"Not encrypted";

        let result = service.decrypt(plaintext);
        assert!(matches!(result, Err(EncryptionError::NotEncrypted)));
    }

    #[test]
    fn test_key_derivation() {
        let password = "my_secure_password";
        let salt = [0u8; SALT_SIZE];

        let key1 = AesGcmEncryption::derive_key(password, &salt).expect("Key derivation should succeed");
        let key2 = AesGcmEncryption::derive_key(password, &salt).expect("Key derivation should succeed");

        // Same password and salt should produce same key
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_different_passwords_produce_different_keys() {
        let salt = [0u8; SALT_SIZE];

        let key1 = AesGcmEncryption::derive_key("password1", &salt).expect("Key derivation should succeed");
        let key2 = AesGcmEncryption::derive_key("password2", &salt).expect("Key derivation should succeed");

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_encrypt_string_utility() {
        let mut key = [0u8; KEY_SIZE];
        rand::thread_rng().fill_bytes(&mut key);

        let plaintext = "Test string to encrypt";
        let encrypted = encrypt_string(plaintext, &key).expect("Encryption should succeed");
        let decrypted = decrypt_string(&encrypted, &key).expect("Decryption should succeed");

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_encryption_settings() {
        let service = create_test_service();
        let settings = service.settings();

        assert!(settings.enabled);
        assert!(settings.key_info.is_some());
    }

    #[tokio::test]
    async fn test_key_manager_enable_disable() {
        let manager = EncryptionKeyManager::new();

        // Initially disabled
        assert!(!manager.is_enabled().await);

        // Enable encryption
        manager
            .enable_encryption("test_password")
            .await
            .expect("Enable should succeed");

        assert!(manager.is_enabled().await);

        // Disable encryption
        manager
            .disable_encryption()
            .await
            .expect("Disable should succeed");

        assert!(!manager.is_enabled().await);
    }

    #[tokio::test]
    async fn test_key_manager_encrypt_decrypt() {
        let manager = EncryptionKeyManager::new();

        manager
            .enable_encryption("test_password")
            .await
            .expect("Enable should succeed");

        let plaintext = b"Secret data";
        let encrypted = manager.encrypt(plaintext).await.expect("Encrypt should succeed");
        let decrypted = manager.decrypt(&encrypted).await.expect("Decrypt should succeed");

        assert_eq!(plaintext.to_vec(), decrypted);
    }
}