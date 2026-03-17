//! Encrypted database layer for sensitive field protection
//!
//! This module provides transparent encryption/decryption for sensitive database fields
//! using the EncryptionKeyManager from the security module.
//!
//! # Encrypted Fields
//! - `messages.content` - User conversation content
//! - `agents.system_prompt` - Agent system prompts
//!
//! # Migration
//! Migration 004 adds the `encrypted_fields` metadata table to track which
//! records have encrypted data.

use anyhow::{Context, Result};
use std::sync::Arc;

use crate::security::EncryptionKeyManager;

/// Prefix marker for encrypted data (base64 encoded encrypted data)
const ENCRYPTION_PREFIX: &str = "ENCv1:";

/// Check if a value is encrypted by looking for the prefix
pub fn is_encrypted_value(value: &str) -> bool {
    value.starts_with(ENCRYPTION_PREFIX)
}

/// Encrypted database layer wrapper
///
/// Provides transparent encryption/decryption for sensitive fields.
/// This is designed to be used alongside the regular database operations.
pub struct EncryptedDb {
    /// Key manager for encryption operations
    key_manager: Arc<EncryptionKeyManager>,
}

impl EncryptedDb {
    /// Create a new encrypted database layer
    ///
    /// # Arguments
    /// * `key_manager` - The key manager for encryption operations
    pub fn new(key_manager: Arc<EncryptionKeyManager>) -> Self {
        Self { key_manager }
    }

    /// Check if encryption is currently enabled
    pub async fn is_encryption_enabled(&self) -> bool {
        self.key_manager.is_enabled().await
    }

    /// Encrypt a field value if encryption is enabled
    ///
    /// Returns the encrypted value with prefix, or the original value if encryption is disabled.
    ///
    /// # Arguments
    /// * `value` - The plaintext value to encrypt
    ///
    /// # Returns
    /// - If encryption enabled: "ENCv1:<base64_encoded_ciphertext>"
    /// - If encryption disabled: original plaintext value
    pub async fn encrypt_field(&self, value: &str) -> Result<String> {
        if !self.is_encryption_enabled().await {
            return Ok(value.to_string());
        }

        if value.is_empty() {
            return Ok(value.to_string());
        }

        // Don't re-encrypt already encrypted values
        if is_encrypted_value(value) {
            return Ok(value.to_string());
        }

        let encrypted = self.key_manager.encrypt(value.as_bytes()).await?;
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encrypted);
        Ok(format!("{}{}", ENCRYPTION_PREFIX, encoded))
    }

    /// Decrypt a field value
    ///
    /// # Arguments
    /// * `value` - The potentially encrypted value
    ///
    /// # Returns
    /// - If value is encrypted and encryption is enabled: decrypted plaintext
    /// - If value is encrypted but encryption is disabled: error (cannot decrypt)
    /// - If value is not encrypted: original value unchanged
    pub async fn decrypt_field(&self, value: &str) -> Result<String> {
        if !is_encrypted_value(value) {
            return Ok(value.to_string());
        }

        // Value is encrypted, need to decrypt
        let encoded = value.strip_prefix(ENCRYPTION_PREFIX).context("Invalid encryption prefix")?;
        let encrypted = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
            .context("Failed to decode base64 encrypted data")?;

        let decrypted = self.key_manager.decrypt(&encrypted).await?;
        String::from_utf8(decrypted).context("Decrypted data is not valid UTF-8")
    }

    /// Decrypt a field value, returning the encrypted marker if decryption fails
    ///
    /// This is useful for display purposes where you want to show something
    /// even if decryption fails.
    pub async fn decrypt_field_or_marker(&self, value: &str) -> String {
        match self.decrypt_field(value).await {
            Ok(decrypted) => decrypted,
            Err(_) => "[ENCRYPTED]".to_string(),
        }
    }

    /// Get the key manager
    pub fn key_manager(&self) -> &Arc<EncryptionKeyManager> {
        &self.key_manager
    }
}

/// Encrypted field metadata for tracking encrypted records
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncryptedFieldMetadata {
    /// Table name
    pub table_name: String,
    /// Column name
    pub column_name: String,
    /// Record ID
    pub record_id: i64,
    /// Encryption version/key ID
    pub key_id: String,
    /// When it was encrypted
    pub encrypted_at: i64,
}

impl EncryptedFieldMetadata {
    /// Create new metadata for an encrypted field
    pub fn new(table: &str, column: &str, record_id: i64, key_id: &str) -> Self {
        Self {
            table_name: table.to_string(),
            column_name: column.to_string(),
            record_id,
            key_id: key_id.to_string(),
            encrypted_at: chrono::Utc::now().timestamp(),
        }
    }
}

/// Encrypted fields registry
///
/// Tracks which database fields contain encrypted data.
/// This information is stored in the `encrypted_fields` table.
pub struct EncryptedFieldsRegistry;

impl EncryptedFieldsRegistry {
    /// Get the list of tables and columns that can be encrypted
    pub fn encryptable_fields() -> &'static [(&'static str, &'static str)] {
        &[
            ("messages", "content"),
            ("agents", "system_prompt"),
        ]
    }

    /// Check if a field is encryptable
    pub fn is_encryptable(table: &str, column: &str) -> bool {
        Self::encryptable_fields()
            .iter()
            .any(|(t, c)| *t == table && *c == column)
    }
}

/// Migration SQL for encrypted fields metadata table
pub const ENCRYPTED_FIELDS_MIGRATION_SQL: &str = r#"
-- Migration: 004_encrypted_fields
-- Description: Add encrypted_fields table for tracking encrypted data

CREATE TABLE IF NOT EXISTS encrypted_fields (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    table_name TEXT NOT NULL,
    column_name TEXT NOT NULL,
    record_id INTEGER NOT NULL,
    key_id TEXT NOT NULL,
    encrypted_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(table_name, column_name, record_id)
);

-- Index for looking up encrypted fields by table/column
CREATE INDEX IF NOT EXISTS idx_encrypted_fields_table_column
    ON encrypted_fields(table_name, column_name);

-- Index for looking up by record ID
CREATE INDEX IF NOT EXISTS idx_encrypted_fields_record
    ON encrypted_fields(table_name, record_id);
"#;

/// Rollback SQL for encrypted fields migration
pub const ENCRYPTED_FIELDS_MIGRATION_DOWN_SQL: &str = r#"
-- Rollback: 004_encrypted_fields
DROP INDEX IF EXISTS idx_encrypted_fields_record;
DROP INDEX IF EXISTS idx_encrypted_fields_table_column;
DROP TABLE IF EXISTS encrypted_fields;
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_encrypted_value() {
        assert!(is_encrypted_value("ENCv1:YWJjZGVm"));
        assert!(!is_encrypted_value("regular text"));
        assert!(!is_encrypted_value(""));
        assert!(!is_encrypted_value("ENCv1"));  // No colon
    }

    #[tokio::test]
    async fn test_encrypt_field_when_disabled() {
        let key_manager = Arc::new(EncryptionKeyManager::new());
        let db = EncryptedDb::new(key_manager);

        let result = db.encrypt_field("test value").await.expect("Encryption should succeed");
        assert_eq!(result, "test value");
    }

    #[tokio::test]
    async fn test_decrypt_field_unencrypted() {
        let key_manager = Arc::new(EncryptionKeyManager::new());
        let db = EncryptedDb::new(key_manager);

        let result = db.decrypt_field("plain text").await.expect("Decryption should succeed");
        assert_eq!(result, "plain text");
    }

    #[tokio::test]
    async fn test_encryption_roundtrip() {
        let key_manager = Arc::new(EncryptionKeyManager::new());

        // Enable encryption first
        key_manager.enable_encryption("test_password").await.expect("Failed to enable encryption");

        let db = EncryptedDb::new(key_manager);

        let original = "This is a secret message";
        let encrypted = db.encrypt_field(original).await.expect("Encryption failed");

        // Should have the prefix
        assert!(is_encrypted_value(&encrypted));
        assert_ne!(encrypted, original);

        // Decrypt should return original
        let decrypted = db.decrypt_field(&encrypted).await.expect("Decryption failed");
        assert_eq!(decrypted, original);
    }

    #[tokio::test]
    async fn test_encrypt_empty_value() {
        let key_manager = Arc::new(EncryptionKeyManager::new());
        key_manager.enable_encryption("test_password").await.expect("Failed to enable encryption");

        let db = EncryptedDb::new(key_manager);

        let result = db.encrypt_field("").await.expect("Should handle empty string");
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn test_encrypt_already_encrypted() {
        let key_manager = Arc::new(EncryptionKeyManager::new());
        key_manager.enable_encryption("test_password").await.expect("Failed to enable encryption");

        let db = EncryptedDb::new(key_manager);

        let already_encrypted = "ENCv1:c29tZWRhdGE=";
        let result = db.encrypt_field(already_encrypted).await.expect("Should handle already encrypted");
        assert_eq!(result, already_encrypted);
    }

    #[tokio::test]
    async fn test_decrypt_when_encryption_disabled_returns_marker() {
        let key_manager = Arc::new(EncryptionKeyManager::new());

        // Enable and encrypt
        key_manager.enable_encryption("test_password").await.expect("Failed to enable encryption");
        let db = EncryptedDb::new(key_manager.clone());
        let encrypted = db.encrypt_field("secret").await.expect("Encryption failed");

        // Disable encryption
        key_manager.disable_encryption().await.expect("Failed to disable encryption");

        // Now decrypt should return marker
        let result = db.decrypt_field_or_marker(&encrypted).await;
        assert_eq!(result, "[ENCRYPTED]");
    }

    #[test]
    fn test_encrypted_field_metadata() {
        let meta = EncryptedFieldMetadata::new("messages", "content", 42, "key_v1");
        assert_eq!(meta.table_name, "messages");
        assert_eq!(meta.column_name, "content");
        assert_eq!(meta.record_id, 42);
        assert_eq!(meta.key_id, "key_v1");
        assert!(meta.encrypted_at > 0);
    }

    #[test]
    fn test_encryptable_fields() {
        assert!(EncryptedFieldsRegistry::is_encryptable("messages", "content"));
        assert!(EncryptedFieldsRegistry::is_encryptable("agents", "system_prompt"));
        assert!(!EncryptedFieldsRegistry::is_encryptable("agents", "name"));
        assert!(!EncryptedFieldsRegistry::is_encryptable("sessions", "title"));
    }
}