//! OS Keychain integration for secure API key storage.
//!
//! Provides cross-platform secure storage for API keys using:
//! - macOS: Keychain Access
//! - Windows: Credential Manager
//! - Linux: Secret Service API (gnome-keyring/KWallet)
//!
//! Fallback to encrypted file storage when keychain is unavailable.
//!
//! [Source: 3-5-os-keychain-integration.md]

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::sync::RwLock;

use super::crypto::{AesGcmEncryption, EncryptionError, EncryptionService};

// ============================================================================
// Constants
// ============================================================================

/// Service name for keychain entries
const KEYCHAIN_SERVICE: &str = "com.omninoval.app";

/// Key reference URL scheme
const KEYRING_SCHEME: &str = "keyring://";

/// Fallback storage directory name
const FALLBACK_DIR: &str = "secrets";

/// Fallback storage file extension
const FALLBACK_EXT: &str = ".enc";

// ============================================================================
// Error Types
// ============================================================================

/// Keyring-related errors with Chinese messages
#[derive(Debug, Error)]
pub enum KeyringError {
    /// Keychain access failed
    #[error("密钥链访问失败：{0}")]
    AccessFailed(String),

    /// Failed to save key
    #[error("密钥存储失败：无法保存 API 密钥到系统密钥链")]
    SaveFailed(String),

    /// Key not found
    #[error("密钥检索失败：找不到指定的 API 密钥（{0}）")]
    KeyNotFound(String),

    /// Failed to delete key
    #[error("密钥删除失败：无法从系统密钥链删除密钥")]
    DeleteFailed(String),

    /// Keychain is locked
    #[error("密钥链被锁定：请解锁系统密钥链后重试")]
    KeychainLocked,

    /// Permission denied
    #[error("权限不足：无法访问系统密钥链")]
    PermissionDenied,

    /// Keychain unavailable, using fallback
    #[error("密钥链不可用：{0}，已使用加密文件存储作为回退")]
    KeychainUnavailable(String),

    /// Invalid key reference format
    #[error("无效的密钥引用格式：{0}")]
    InvalidReference(String),

    /// Encryption error
    #[error("加密错误：{0}")]
    EncryptionError(#[from] EncryptionError),

    /// IO error
    #[error("文件操作错误：{0}")]
    IoError(String),

    /// Key already exists
    #[error("密钥已存在：{0}")]
    KeyAlreadyExists(String),
}

// ============================================================================
// Key Reference
// ============================================================================

/// Represents a reference to a stored secret
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyReference {
    /// Category (e.g., "providers", "channels")
    pub category: String,
    /// Provider or service name
    pub name: String,
    /// Key type (e.g., "api_key", "bot_token")
    pub key_type: String,
}

impl KeyReference {
    /// Create a new key reference
    pub fn new(category: impl Into<String>, name: impl Into<String>, key_type: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            name: name.into(),
            key_type: key_type.into(),
        }
    }

    /// Parse from URL string
    /// Format: keyring://category/name/key_type
    pub fn parse(url: &str) -> Result<Self, KeyringError> {
        if !url.starts_with(KEYRING_SCHEME) {
            return Err(KeyringError::InvalidReference(url.to_string()));
        }

        let path = &url[KEYRING_SCHEME.len()..];
        let parts: Vec<&str> = path.split('/').collect();

        if parts.len() != 3 {
            return Err(KeyringError::InvalidReference(url.to_string()));
        }

        Ok(Self {
            category: parts[0].to_string(),
            name: parts[1].to_string(),
            key_type: parts[2].to_string(),
        })
    }

    /// Convert to URL string
    pub fn to_url(&self) -> String {
        format!("{}{}/{}/{}", KEYRING_SCHEME, self.category, self.name, self.key_type)
    }

    /// Get the unique key identifier for keychain storage
    pub fn key_id(&self) -> String {
        format!("{}_{}_{}", self.category, self.name, self.key_type)
    }
}

impl std::fmt::Display for KeyReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_url())
    }
}

// ============================================================================
// Secret Store Trait
// ============================================================================

/// Trait for secret storage backends
#[async_trait::async_trait]
pub trait SecretStore: Send + Sync {
    /// Store a secret
    async fn save(&self, reference: &KeyReference, secret: &str) -> Result<(), KeyringError>;

    /// Retrieve a secret
    async fn get(&self, reference: &KeyReference) -> Result<String, KeyringError>;

    /// Delete a secret
    async fn delete(&self, reference: &KeyReference) -> Result<(), KeyringError>;

    /// Check if a secret exists
    async fn exists(&self, reference: &KeyReference) -> Result<bool, KeyringError>;

    /// Get the store name/type
    fn store_type(&self) -> &'static str;
}

// ============================================================================
// OS Keyring Implementation
// ============================================================================

/// OS-native keyring storage
pub struct OsKeyring {
    /// Whether the keyring is available
    available: bool,
}

impl OsKeyring {
    /// Create a new OS keyring instance
    pub fn new() -> Self {
        // Test if keyring is available by trying a test operation
        let available = Self::test_availability();
        Self { available }
    }

    /// Test if the OS keyring is available
    fn test_availability() -> bool {
        // Try to create a test entry to verify keychain access
        let test_entry = keyring::Entry::new(KEYCHAIN_SERVICE, "__test_availability__");
        match test_entry {
            Ok(entry) => {
                // Try to get password (will fail if doesn't exist, but that's OK)
                let _ = entry.get_password();
                true
            }
            Err(_) => false,
        }
    }

    /// Check if keyring is available
    pub fn is_available(&self) -> bool {
        self.available
    }
}

impl Default for OsKeyring {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SecretStore for OsKeyring {
    async fn save(&self, reference: &KeyReference, secret: &str) -> Result<(), KeyringError> {
        if !self.available {
            return Err(KeyringError::KeychainUnavailable(
                "OS keyring not available".to_string(),
            ));
        }

        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &reference.key_id())
            .map_err(|e| KeyringError::SaveFailed(e.to_string()))?;

        entry
            .set_password(secret)
            .map_err(|e| {
                // Detect specific error types
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("locked") || err_str.contains("unlock") {
                    KeyringError::KeychainLocked
                } else if err_str.contains("permission") || err_str.contains("denied") {
                    KeyringError::PermissionDenied
                } else {
                    KeyringError::SaveFailed(e.to_string())
                }
            })
    }

    async fn get(&self, reference: &KeyReference) -> Result<String, KeyringError> {
        if !self.available {
            return Err(KeyringError::KeychainUnavailable(
                "OS keyring not available".to_string(),
            ));
        }

        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &reference.key_id())
            .map_err(|e| KeyringError::AccessFailed(e.to_string()))?;

        entry.get_password().map_err(|e| {
            let err_str = e.to_string().to_lowercase();
            if err_str.contains("not found") || err_str.contains("no matching") {
                KeyringError::KeyNotFound(reference.to_url())
            } else if err_str.contains("locked") || err_str.contains("unlock") {
                KeyringError::KeychainLocked
            } else if err_str.contains("permission") || err_str.contains("denied") {
                KeyringError::PermissionDenied
            } else {
                KeyringError::AccessFailed(e.to_string())
            }
        })
    }

    async fn delete(&self, reference: &KeyReference) -> Result<(), KeyringError> {
        if !self.available {
            return Err(KeyringError::KeychainUnavailable(
                "OS keyring not available".to_string(),
            ));
        }

        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &reference.key_id())
            .map_err(|e| KeyringError::DeleteFailed(e.to_string()))?;

        entry.delete_credential().map_err(|e| {
            let err_str = e.to_string().to_lowercase();
            if err_str.contains("not found") || err_str.contains("no matching") {
                KeyringError::KeyNotFound(reference.to_url())
            } else {
                KeyringError::DeleteFailed(e.to_string())
            }
        })
    }

    async fn exists(&self, reference: &KeyReference) -> Result<bool, KeyringError> {
        if !self.available {
            return Ok(false);
        }

        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &reference.key_id())
            .map_err(|e| KeyringError::AccessFailed(e.to_string()))?;

        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("not found") || err_str.contains("no matching") {
                    Ok(false)
                } else {
                    Err(KeyringError::AccessFailed(e.to_string()))
                }
            }
        }
    }

    fn store_type(&self) -> &'static str {
        "os-keyring"
    }
}

// ============================================================================
// Fallback Encrypted File Storage
// ============================================================================

/// Encrypted file-based fallback storage
pub struct FallbackStorage {
    /// Base directory for storage
    base_dir: PathBuf,
    /// Encryption key (derived from machine-specific data)
    encryption_key: [u8; 32],
}

impl FallbackStorage {
    /// Create a new fallback storage instance
    pub async fn new() -> Result<Self, KeyringError> {
        let config_dir = directories::ProjectDirs::from("com", "omninoval", "omninoval")
            .ok_or_else(|| KeyringError::IoError("Cannot find config directory".to_string()))?;

        let base_dir = config_dir.data_dir().join(FALLBACK_DIR);

        // Create directory if it doesn't exist
        fs::create_dir_all(&base_dir)
            .await
            .map_err(|e| KeyringError::IoError(e.to_string()))?;

        // Derive encryption key from machine-specific data
        let encryption_key = Self::derive_machine_key()?;

        Ok(Self {
            base_dir,
            encryption_key,
        })
    }

    /// Derive an encryption key from machine-specific data
    fn derive_machine_key() -> Result<[u8; 32], KeyringError> {
        use std::env;

        // Combine machine-identifying information
        let mut seed = String::new();

        // Add hostname
        if let Ok(hostname) = env::var("HOSTNAME").or_else(|_| env::var("COMPUTERNAME")) {
            seed.push_str(&hostname);
        }

        // Add home directory path
        if let Some(home) = home::home_dir() {
            seed.push_str(&home.to_string_lossy());
        }

        // Add user name
        if let Ok(user) = env::var("USER").or_else(|_| env::var("USERNAME")) {
            seed.push_str(&user);
        }

        // If we don't have enough seed data, use a default
        if seed.is_empty() {
            seed = "omninoval-default-key-seed".to_string();
        }

        // Derive key using SHA-256
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        let hash = hasher.finalize();

        let mut key = [0u8; 32];
        key.copy_from_slice(&hash);

        Ok(key)
    }

    /// Get file path for a key reference
    fn get_file_path(&self, reference: &KeyReference) -> PathBuf {
        // Use base64-encoded key ID to avoid filesystem issues
        let encoded_id = BASE64.encode(reference.key_id().as_bytes());
        self.base_dir.join(format!("{}{}", encoded_id, FALLBACK_EXT))
    }
}

#[async_trait::async_trait]
impl SecretStore for FallbackStorage {
    async fn save(&self, reference: &KeyReference, secret: &str) -> Result<(), KeyringError> {
        let file_path = self.get_file_path(reference);

        // Encrypt the secret
        let encryption = AesGcmEncryption::from_key(self.encryption_key);
        let encrypted = encryption.encrypt(secret.as_bytes())?;

        // Write to file
        fs::write(&file_path, encrypted)
            .await
            .map_err(|e| KeyringError::IoError(e.to_string()))?;

        Ok(())
    }

    async fn get(&self, reference: &KeyReference) -> Result<String, KeyringError> {
        let file_path = self.get_file_path(reference);

        if !file_path.exists() {
            return Err(KeyringError::KeyNotFound(reference.to_url()));
        }

        // Read encrypted data
        let encrypted = fs::read(&file_path)
            .await
            .map_err(|e| KeyringError::IoError(e.to_string()))?;

        // Decrypt
        let encryption = AesGcmEncryption::from_key(self.encryption_key);
        let decrypted = encryption.decrypt(&encrypted)?;

        String::from_utf8(decrypted).map_err(|_| {
            KeyringError::EncryptionError(EncryptionError::DecryptionFailed(
                "Invalid UTF-8".to_string(),
            ))
        })
    }

    async fn delete(&self, reference: &KeyReference) -> Result<(), KeyringError> {
        let file_path = self.get_file_path(reference);

        if !file_path.exists() {
            return Err(KeyringError::KeyNotFound(reference.to_url()));
        }

        fs::remove_file(&file_path)
            .await
            .map_err(|e| KeyringError::IoError(e.to_string()))?;

        Ok(())
    }

    async fn exists(&self, reference: &KeyReference) -> Result<bool, KeyringError> {
        let file_path = self.get_file_path(reference);
        Ok(file_path.exists())
    }

    fn store_type(&self) -> &'static str {
        "encrypted-file"
    }
}

// ============================================================================
// Hybrid Secret Store
// ============================================================================

/// Hybrid storage that uses OS keyring with fallback to encrypted file storage
pub struct HybridSecretStore {
    /// OS keyring (if available)
    keyring: Option<OsKeyring>,
    /// Fallback encrypted storage
    fallback: Arc<RwLock<Option<FallbackStorage>>>,
    /// Whether to use fallback
    use_fallback: bool,
}

impl HybridSecretStore {
    /// Create a new hybrid secret store
    pub fn new() -> Self {
        let keyring = OsKeyring::new();
        let available = keyring.is_available();

        if !available {
            tracing::warn!(
                "OS keyring not available, will use encrypted file storage as fallback"
            );
        }

        Self {
            keyring: if available { Some(keyring) } else { None },
            fallback: Arc::new(RwLock::new(None)),
            use_fallback: !available,
        }
    }

    /// Create with forced fallback (for testing)
    pub fn new_with_fallback() -> Self {
        Self {
            keyring: None,
            fallback: Arc::new(RwLock::new(None)),
            use_fallback: true,
        }
    }

    /// Initialize fallback storage lazily
    async fn get_fallback(&self) -> Result<FallbackStorage, KeyringError> {
        let mut fallback = self.fallback.write().await;
        if fallback.is_none() {
            *fallback = Some(FallbackStorage::new().await?);
        }
        Ok(fallback.as_ref().unwrap().clone())
    }

    /// Check if using fallback storage
    pub fn is_using_fallback(&self) -> bool {
        self.use_fallback
    }

    /// Migrate a key from fallback to keyring
    pub async fn migrate_to_keyring(&self, reference: &KeyReference) -> Result<(), KeyringError> {
        if !self.use_fallback {
            return Ok(());
        }

        // Get from fallback
        let fallback = self.get_fallback().await?;
        let secret = fallback.get(reference).await?;

        // Try to save to keyring
        if let Some(ref keyring) = self.keyring {
            if keyring.is_available() {
                keyring.save(reference, &secret).await?;
                // Delete from fallback after successful migration
                fallback.delete(reference).await?;
                tracing::info!("Migrated key {} from fallback to keyring", reference.to_url());
            }
        }

        Ok(())
    }
}

impl Default for HybridSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SecretStore for HybridSecretStore {
    async fn save(&self, reference: &KeyReference, secret: &str) -> Result<(), KeyringError> {
        if self.use_fallback {
            let fallback = self.get_fallback().await?;
            fallback.save(reference, secret).await
        } else if let Some(ref keyring) = self.keyring {
            keyring.save(reference, secret).await
        } else {
            let fallback = self.get_fallback().await?;
            fallback.save(reference, secret).await
        }
    }

    async fn get(&self, reference: &KeyReference) -> Result<String, KeyringError> {
        if self.use_fallback {
            let fallback = self.get_fallback().await?;
            fallback.get(reference).await
        } else if let Some(ref keyring) = self.keyring {
            keyring.get(reference).await
        } else {
            let fallback = self.get_fallback().await?;
            fallback.get(reference).await
        }
    }

    async fn delete(&self, reference: &KeyReference) -> Result<(), KeyringError> {
        if self.use_fallback {
            let fallback = self.get_fallback().await?;
            fallback.delete(reference).await
        } else if let Some(ref keyring) = self.keyring {
            keyring.delete(reference).await
        } else {
            let fallback = self.get_fallback().await?;
            fallback.delete(reference).await
        }
    }

    async fn exists(&self, reference: &KeyReference) -> Result<bool, KeyringError> {
        if self.use_fallback {
            let fallback = self.get_fallback().await?;
            fallback.exists(reference).await
        } else if let Some(ref keyring) = self.keyring {
            keyring.exists(reference).await
        } else {
            let fallback = self.get_fallback().await?;
            fallback.exists(reference).await
        }
    }

    fn store_type(&self) -> &'static str {
        if self.use_fallback {
            "encrypted-file"
        } else {
            "os-keyring"
        }
    }
}

// Make FallbackStorage cloneable (needed for HybridSecretStore)
impl Clone for FallbackStorage {
    fn clone(&self) -> Self {
        Self {
            base_dir: self.base_dir.clone(),
            encryption_key: self.encryption_key,
        }
    }
}

// ============================================================================
// Keyring Service
// ============================================================================

/// Main keyring service for managing API keys
pub struct KeyringService {
    /// The underlying secret store
    store: Arc<dyn SecretStore>,
}

impl KeyringService {
    /// Create a new keyring service with hybrid storage
    pub fn new() -> Self {
        Self {
            store: Arc::new(HybridSecretStore::new()),
        }
    }

    /// Create with a specific store (for testing)
    pub fn with_store(store: Arc<dyn SecretStore>) -> Self {
        Self { store }
    }

    /// Save an API key for a provider
    pub async fn save_provider_key(
        &self,
        provider: &str,
        api_key: &str,
    ) -> Result<KeyReference, KeyringError> {
        let reference = KeyReference::new("providers", provider, "api_key");
        self.store.save(&reference, api_key).await?;
        Ok(reference)
    }

    /// Get an API key for a provider
    pub async fn get_provider_key(&self, provider: &str) -> Result<String, KeyringError> {
        let reference = KeyReference::new("providers", provider, "api_key");
        self.store.get(&reference).await
    }

    /// Delete an API key for a provider
    pub async fn delete_provider_key(&self, provider: &str) -> Result<(), KeyringError> {
        let reference = KeyReference::new("providers", provider, "api_key");
        self.store.delete(&reference).await
    }

    /// Check if an API key exists for a provider
    pub async fn provider_key_exists(&self, provider: &str) -> Result<bool, KeyringError> {
        let reference = KeyReference::new("providers", provider, "api_key");
        self.store.exists(&reference).await
    }

    /// Save a secret with a custom reference
    pub async fn save_secret(
        &self,
        reference: &KeyReference,
        secret: &str,
    ) -> Result<(), KeyringError> {
        self.store.save(reference, secret).await
    }

    /// Get a secret by reference
    pub async fn get_secret(&self, reference: &KeyReference) -> Result<String, KeyringError> {
        self.store.get(reference).await
    }

    /// Delete a secret by reference
    pub async fn delete_secret(&self, reference: &KeyReference) -> Result<(), KeyringError> {
        self.store.delete(reference).await
    }

    /// Check if a secret exists
    pub async fn secret_exists(&self, reference: &KeyReference) -> Result<bool, KeyringError> {
        self.store.exists(reference).await
    }

    /// Get the store type being used
    pub fn store_type(&self) -> &'static str {
        self.store.store_type()
    }
}

impl Default for KeyringService {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_reference_creation() {
        let reference = KeyReference::new("providers", "openai", "api_key");
        assert_eq!(reference.category, "providers");
        assert_eq!(reference.name, "openai");
        assert_eq!(reference.key_type, "api_key");
    }

    #[test]
    fn test_key_reference_to_url() {
        let reference = KeyReference::new("providers", "openai", "api_key");
        assert_eq!(
            reference.to_url(),
            "keyring://providers/openai/api_key"
        );
    }

    #[test]
    fn test_key_reference_parse() {
        let reference = KeyReference::parse("keyring://providers/openai/api_key").unwrap();
        assert_eq!(reference.category, "providers");
        assert_eq!(reference.name, "openai");
        assert_eq!(reference.key_type, "api_key");
    }

    #[test]
    fn test_key_reference_parse_invalid() {
        // Missing scheme
        assert!(KeyReference::parse("providers/openai/api_key").is_err());

        // Wrong number of parts
        assert!(KeyReference::parse("keyring://providers/openai").is_err());
        assert!(KeyReference::parse("keyring://providers/openai/api_key/extra").is_err());
    }

    #[test]
    fn test_key_reference_key_id() {
        let reference = KeyReference::new("providers", "openai", "api_key");
        assert_eq!(reference.key_id(), "providers_openai_api_key");
    }

    #[tokio::test]
    async fn test_fallback_storage_roundtrip() {
        // Create a fallback storage in temp directory
        let temp_dir = tempfile::tempdir().unwrap();
        let mut storage = FallbackStorage::new().await.unwrap();
        storage.base_dir = temp_dir.path().to_path_buf();

        let reference = KeyReference::new("providers", "test_provider", "api_key");
        let secret = "sk-test-secret-key-12345";

        // Save
        storage.save(&reference, secret).await.unwrap();

        // Get
        let retrieved = storage.get(&reference).await.unwrap();
        assert_eq!(retrieved, secret);

        // Exists
        assert!(storage.exists(&reference).await.unwrap());

        // Delete
        storage.delete(&reference).await.unwrap();

        // No longer exists
        assert!(!storage.exists(&reference).await.unwrap());
    }

    #[tokio::test]
    async fn test_fallback_storage_key_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut storage = FallbackStorage::new().await.unwrap();
        storage.base_dir = temp_dir.path().to_path_buf();

        let reference = KeyReference::new("providers", "nonexistent", "api_key");

        let result = storage.get(&reference).await;
        assert!(matches!(result, Err(KeyringError::KeyNotFound(_))));
    }

    #[tokio::test]
    async fn test_hybrid_store_with_fallback() {
        // Create hybrid store with forced fallback
        let store = HybridSecretStore::new_with_fallback();

        assert!(store.is_using_fallback());
        assert_eq!(store.store_type(), "encrypted-file");

        let reference = KeyReference::new("providers", "test", "api_key");
        let secret = "test-secret";

        // Save
        store.save(&reference, secret).await.unwrap();

        // Get
        let retrieved = store.get(&reference).await.unwrap();
        assert_eq!(retrieved, secret);

        // Exists
        assert!(store.exists(&reference).await.unwrap());

        // Delete
        store.delete(&reference).await.unwrap();
        assert!(!store.exists(&reference).await.unwrap());
    }

    #[tokio::test]
    async fn test_keyring_service_provider_operations() {
        let store = Arc::new(HybridSecretStore::new_with_fallback());
        let service = KeyringService::with_store(store);

        // Save provider key
        let reference = service.save_provider_key("test_provider", "sk-test-key").await.unwrap();
        assert_eq!(reference.name, "test_provider");

        // Get provider key
        let key = service.get_provider_key("test_provider").await.unwrap();
        assert_eq!(key, "sk-test-key");

        // Check exists
        assert!(service.provider_key_exists("test_provider").await.unwrap());

        // Delete
        service.delete_provider_key("test_provider").await.unwrap();
        assert!(!service.provider_key_exists("test_provider").await.unwrap());
    }

    #[test]
    fn test_error_messages_are_chinese() {
        let err = KeyringError::AccessFailed("test".to_string());
        assert!(err.to_string().contains("密钥链访问失败"));

        let err = KeyringError::KeyNotFound("test-key".to_string());
        assert!(err.to_string().contains("找不到"));

        let err = KeyringError::KeychainLocked;
        assert!(err.to_string().contains("被锁定"));

        let err = KeyringError::PermissionDenied;
        assert!(err.to_string().contains("权限不足"));
    }
}