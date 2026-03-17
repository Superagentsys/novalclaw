//! Account storage layer for database operations.
//!
//! This module provides CRUD operations for Account records
//! using the SQLite connection pool.
//!
//! [Source: 2-11-local-account-management.md]

use crate::account::{Account, AccountInfo, AccountUpdate, NewAccount};
use crate::db::{DbConnection, DbPool};
use crate::security::password::{hash_password, verify_password, validate_password_strength};
use anyhow::Result;
use rusqlite::params;
use thiserror::Error;

/// Error type for account operations
#[derive(Debug, Error)]
pub enum AccountStoreError {
    /// Account not found
    #[error("账户不存在")]
    NotFound,

    /// Database error
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    /// Pool error
    #[error("连接池错误: {0}")]
    Pool(String),

    /// Password error
    #[error("密码错误")]
    InvalidPassword,

    /// Password validation error
    #[error("密码强度不足: {0}")]
    PasswordValidation(String),

    /// Account already exists
    #[error("账户已存在")]
    AlreadyExists,

    /// Hashing error
    #[error("密码哈希失败: {0}")]
    HashError(#[from] crate::security::password::PasswordError),
}

/// Account storage handler
#[derive(Clone)]
pub struct AccountStore {
    pool: DbPool,
}

impl AccountStore {
    /// Create a new AccountStore with the given connection pool
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Get a connection from the pool
    fn get_conn(&self) -> Result<DbConnection, AccountStoreError> {
        self.pool.get().map_err(|e| AccountStoreError::Pool(e.to_string()))
    }

    /// Get current timestamp in seconds
    fn current_timestamp() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// Create a new account (single user mode, only one account allowed)
    ///
    /// # Arguments
    /// * `new_account` - The new account data
    ///
    /// # Returns
    /// The created account
    ///
    /// # Errors
    /// - `AlreadyExists` if an account already exists
    /// - `PasswordValidation` if password doesn't meet requirements
    pub fn create(&self, new_account: &NewAccount) -> Result<Account, AccountStoreError> {
        // Validate password strength
        validate_password_strength(&new_account.password)?;

        // Check if account already exists
        if self.exists()? {
            return Err(AccountStoreError::AlreadyExists);
        }

        let conn = self.get_conn()?;
        let password_hash = hash_password(&new_account.password)?;
        let timestamp = Self::current_timestamp();

        conn.execute(
            "INSERT INTO accounts (id, username, password_hash, require_password_on_startup, created_at, updated_at)
             VALUES (1, ?1, ?2, 0, ?3, ?4)",
            params![
                &new_account.username,
                &password_hash,
                timestamp,
                timestamp,
            ],
        )?;

        Ok(Account {
            id: 1,
            username: new_account.username.clone(),
            password_hash,
            require_password_on_startup: false,
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    /// Create an account without a password (for backup import)
    ///
    /// The account will have an empty password hash, and the user
    /// will need to set a password later.
    ///
    /// # Arguments
    /// * `username` - The username for the account
    ///
    /// # Returns
    /// The created account
    ///
    /// # Errors
    /// - `AlreadyExists` if an account already exists
    pub fn create_without_password(&self, username: &str) -> Result<Account, AccountStoreError> {
        // Check if account already exists
        if self.exists()? {
            return Err(AccountStoreError::AlreadyExists);
        }

        let conn = self.get_conn()?;
        let timestamp = Self::current_timestamp();

        conn.execute(
            "INSERT INTO accounts (id, username, password_hash, require_password_on_startup, created_at, updated_at)
             VALUES (1, ?1, '', 0, ?2, ?3)",
            params![
                username,
                timestamp,
                timestamp,
            ],
        )?;

        Ok(Account {
            id: 1,
            username: username.to_string(),
            password_hash: String::new(),
            require_password_on_startup: false,
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    /// Get the current account (single user mode)
    ///
    /// # Returns
    /// - `Some(Account)` if account exists
    /// - `None` if no account exists
    pub fn get(&self) -> Result<Option<Account>, AccountStoreError> {
        let conn = self.get_conn()?;
        let result = conn.query_row(
            "SELECT id, username, password_hash, require_password_on_startup, created_at, updated_at
             FROM accounts WHERE id = 1",
            [],
            |row| {
                Ok(Account {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    require_password_on_startup: row.get::<_, i64>(3)? != 0,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(account) => Ok(Some(account)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get account info (without sensitive data)
    ///
    /// # Returns
    /// - `Some(AccountInfo)` if account exists
    /// - `None` if no account exists
    pub fn get_info(&self) -> Result<Option<AccountInfo>, AccountStoreError> {
        Ok(self.get()?.map(|a| a.into()))
    }

    /// Check if an account exists
    pub fn exists(&self) -> Result<bool, AccountStoreError> {
        Ok(self.get()?.is_some())
    }

    /// Verify password against stored hash
    ///
    /// # Arguments
    /// * `password` - The plain text password to verify
    ///
    /// # Returns
    /// `true` if password matches, `false` otherwise
    ///
    /// # Errors
    /// - `NotFound` if no account exists
    pub fn verify_password(&self, password: &str) -> Result<bool, AccountStoreError> {
        let account = self.get()?.ok_or(AccountStoreError::NotFound)?;
        verify_password(password, &account.password_hash).map_err(AccountStoreError::HashError)
    }

    /// Update password
    ///
    /// # Arguments
    /// * `current_password` - The current password for verification
    /// * `new_password` - The new password to set
    ///
    /// # Errors
    /// - `NotFound` if no account exists
    /// - `InvalidPassword` if current password is incorrect
    /// - `PasswordValidation` if new password doesn't meet requirements
    pub fn update_password(
        &self,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), AccountStoreError> {
        // Verify current password
        if !self.verify_password(current_password)? {
            return Err(AccountStoreError::InvalidPassword);
        }

        // Validate new password strength
        validate_password_strength(new_password)?;

        let conn = self.get_conn()?;
        let new_hash = hash_password(new_password)?;
        let timestamp = Self::current_timestamp();

        conn.execute(
            "UPDATE accounts SET password_hash = ?1, updated_at = ?2 WHERE id = 1",
            params![&new_hash, timestamp],
        )?;

        Ok(())
    }

    /// Update account settings
    ///
    /// # Arguments
    /// * `updates` - The fields to update
    ///
    /// # Errors
    /// - `NotFound` if no account exists
    pub fn update(&self, updates: &AccountUpdate) -> Result<Account, AccountStoreError> {
        let existing = self.get()?.ok_or(AccountStoreError::NotFound)?;
        let conn = self.get_conn()?;
        let timestamp = Self::current_timestamp();

        let new_username = updates.username.as_ref().unwrap_or(&existing.username);
        let new_require_startup = updates.require_password_on_startup.unwrap_or(existing.require_password_on_startup);

        // Handle password update if provided
        let new_hash = if let Some(ref new_pwd) = updates.new_password {
            validate_password_strength(new_pwd)?;
            Some(hash_password(new_pwd)?)
        } else {
            None
        };

        conn.execute(
            "UPDATE accounts SET username = ?1, require_password_on_startup = ?2, password_hash = COALESCE(?3, password_hash), updated_at = ?4 WHERE id = 1",
            params![
                new_username,
                new_require_startup as i64,
                new_hash.as_ref(),
                timestamp,
            ],
        )?;

        Ok(Account {
            id: existing.id,
            username: new_username.clone(),
            password_hash: new_hash.unwrap_or(existing.password_hash),
            require_password_on_startup: new_require_startup,
            created_at: existing.created_at,
            updated_at: timestamp,
        })
    }

    /// Set whether to require password on startup
    pub fn set_require_password_on_startup(&self, require: bool) -> Result<(), AccountStoreError> {
        let conn = self.get_conn()?;
        let timestamp = Self::current_timestamp();

        let rows_affected = conn.execute(
            "UPDATE accounts SET require_password_on_startup = ?1, updated_at = ?2 WHERE id = 1",
            params![require as i64, timestamp],
        )?;

        if rows_affected == 0 {
            return Err(AccountStoreError::NotFound);
        }

        Ok(())
    }

    /// Delete the account
    ///
    /// # Note
    /// This is a destructive operation. All account data will be permanently removed.
    pub fn delete(&self) -> Result<(), AccountStoreError> {
        let conn = self.get_conn()?;
        conn.execute("DELETE FROM accounts WHERE id = 1", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_pool, DbPoolConfig};
    use crate::db::migrations::create_builtin_runner;
    use tempfile::tempdir;

    fn create_test_store() -> AccountStore {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");
        let conn = pool.get().expect("Failed to get connection");
        let runner = create_builtin_runner();
        runner.run(&conn).expect("Failed to run migrations");
        AccountStore::new(pool)
    }

    #[test]
    fn test_create_account() {
        let store = create_test_store();

        let new_account = NewAccount {
            username: "testuser".to_string(),
            password: "secure_password_123".to_string(),
        };

        let created = store.create(&new_account).expect("Failed to create account");

        assert_eq!(created.id, 1);
        assert_eq!(created.username, "testuser");
        assert!(!created.password_hash.is_empty());
        assert!(!created.require_password_on_startup);
    }

    #[test]
    fn test_create_account_already_exists() {
        let store = create_test_store();

        store.create(&NewAccount {
            username: "user1".to_string(),
            password: "password123".to_string(),
        }).expect("Failed to create first account");

        let result = store.create(&NewAccount {
            username: "user2".to_string(),
            password: "password456".to_string(),
        });

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AccountStoreError::AlreadyExists));
    }

    #[test]
    fn test_create_account_password_too_short() {
        let store = create_test_store();

        let result = store.create(&NewAccount {
            username: "user".to_string(),
            password: "short".to_string(),
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_get_account() {
        let store = create_test_store();

        // No account initially
        let none = store.get().expect("Query failed");
        assert!(none.is_none());

        // Create account
        store.create(&NewAccount {
            username: "testuser".to_string(),
            password: "password123".to_string(),
        }).expect("Failed to create account");

        // Now should exist
        let account = store.get().expect("Query failed").expect("Account should exist");
        assert_eq!(account.username, "testuser");
    }

    #[test]
    fn test_get_info() {
        let store = create_test_store();

        store.create(&NewAccount {
            username: "testuser".to_string(),
            password: "password123".to_string(),
        }).expect("Failed to create account");

        let info = store.get_info().expect("Query failed").expect("Info should exist");
        assert_eq!(info.username, "testuser");
        assert!(!info.require_password_on_startup);
        // password_hash should not be in AccountInfo
    }

    #[test]
    fn test_exists() {
        let store = create_test_store();

        assert!(!store.exists().expect("Query failed"));

        store.create(&NewAccount {
            username: "user".to_string(),
            password: "password123".to_string(),
        }).expect("Failed to create account");

        assert!(store.exists().expect("Query failed"));
    }

    #[test]
    fn test_verify_password() {
        let store = create_test_store();

        store.create(&NewAccount {
            username: "user".to_string(),
            password: "correct_password".to_string(),
        }).expect("Failed to create account");

        assert!(store.verify_password("correct_password").expect("Verify failed"));
        assert!(!store.verify_password("wrong_password").expect("Verify failed"));
    }

    #[test]
    fn test_verify_password_no_account() {
        let store = create_test_store();

        let result = store.verify_password("any_password");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AccountStoreError::NotFound));
    }

    #[test]
    fn test_update_password() {
        let store = create_test_store();

        store.create(&NewAccount {
            username: "user".to_string(),
            password: "old_password".to_string(),
        }).expect("Failed to create account");

        // Update password
        store.update_password("old_password", "new_password123")
            .expect("Failed to update password");

        // Old password should not work
        assert!(!store.verify_password("old_password").expect("Verify failed"));
        // New password should work
        assert!(store.verify_password("new_password123").expect("Verify failed"));
    }

    #[test]
    fn test_update_password_wrong_current() {
        let store = create_test_store();

        store.create(&NewAccount {
            username: "user".to_string(),
            password: "correct_password".to_string(),
        }).expect("Failed to create account");

        let result = store.update_password("wrong_password", "new_password123");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AccountStoreError::InvalidPassword));
    }

    #[test]
    fn test_set_require_password_on_startup() {
        let store = create_test_store();

        store.create(&NewAccount {
            username: "user".to_string(),
            password: "password123".to_string(),
        }).expect("Failed to create account");

        // Initially false
        let account = store.get().expect("Query failed").expect("Account should exist");
        assert!(!account.require_password_on_startup);

        // Set to true
        store.set_require_password_on_startup(true).expect("Failed to update");

        let account = store.get().expect("Query failed").expect("Account should exist");
        assert!(account.require_password_on_startup);

        // Set back to false
        store.set_require_password_on_startup(false).expect("Failed to update");

        let account = store.get().expect("Query failed").expect("Account should exist");
        assert!(!account.require_password_on_startup);
    }

    #[test]
    fn test_update_account() {
        let store = create_test_store();

        store.create(&NewAccount {
            username: "oldname".to_string(),
            password: "password123".to_string(),
        }).expect("Failed to create account");

        let updates = AccountUpdate {
            username: Some("newname".to_string()),
            require_password_on_startup: Some(true),
            ..Default::default()
        };

        let updated = store.update(&updates).expect("Failed to update");
        assert_eq!(updated.username, "newname");
        assert!(updated.require_password_on_startup);
    }

    #[test]
    fn test_delete_account() {
        let store = create_test_store();

        store.create(&NewAccount {
            username: "user".to_string(),
            password: "password123".to_string(),
        }).expect("Failed to create account");

        assert!(store.exists().expect("Query failed"));

        store.delete().expect("Failed to delete");

        assert!(!store.exists().expect("Query failed"));
    }
}