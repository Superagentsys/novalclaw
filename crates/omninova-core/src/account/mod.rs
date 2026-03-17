//! Account management module for local user authentication.
//!
//! This module provides:
//! - Account data model
//! - Account storage layer (CRUD operations)
//! - Password-protected local account support
//!
//! [Source: 2-11-local-account-management.md]

pub mod store;

pub use store::{AccountStore, AccountStoreError};

use serde::{Deserialize, Serialize};

/// User account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Account ID (single user mode, fixed to 1)
    pub id: i64,
    /// Username
    pub username: String,
    /// Password hash (Argon2id)
    pub password_hash: String,
    /// Whether to require password verification on startup
    pub require_password_on_startup: bool,
    /// Creation timestamp (Unix timestamp)
    pub created_at: i64,
    /// Last update timestamp (Unix timestamp)
    pub updated_at: i64,
}

/// Account information for frontend display (no sensitive data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    /// Username
    pub username: String,
    /// Whether to require password verification on startup
    pub require_password_on_startup: bool,
    /// Creation timestamp (Unix timestamp)
    pub created_at: i64,
    /// Last update timestamp (Unix timestamp)
    pub updated_at: i64,
}

impl From<Account> for AccountInfo {
    fn from(account: Account) -> Self {
        Self {
            username: account.username,
            require_password_on_startup: account.require_password_on_startup,
            created_at: account.created_at,
            updated_at: account.updated_at,
        }
    }
}

/// New account creation data
#[derive(Debug, Clone)]
pub struct NewAccount {
    /// Username
    pub username: String,
    /// Plain text password (will be hashed)
    pub password: String,
}

/// Account update data
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccountUpdate {
    /// New username (optional)
    pub username: Option<String>,
    /// New password (optional, will be hashed)
    pub new_password: Option<String>,
    /// Whether to require password on startup (optional)
    pub require_password_on_startup: Option<bool>,
}