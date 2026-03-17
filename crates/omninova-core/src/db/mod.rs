//! Database module for OmniNova Claw
//!
//! This module provides:
//! - SQLite connection pool with WAL mode
//! - Migration system for schema versioning
//! - Database initialization and management
//! - Encrypted database layer for sensitive fields

pub mod encrypted;
pub mod migrations;
pub mod pool;

pub use encrypted::{
    EncryptedDb, EncryptedFieldMetadata, EncryptedFieldsRegistry,
    ENCRYPTED_FIELDS_MIGRATION_DOWN_SQL, ENCRYPTED_FIELDS_MIGRATION_SQL,
    is_encrypted_value,
};
pub use migrations::{
    create_builtin_runner, get_builtin_migrations, Migration, MigrationReport, MigrationRunner,
    MigrationStatus,
};
pub use pool::{create_pool, DbConnection, DbPool, DbPoolConfig};