//! SQLite connection pool management with WAL mode
//!
//! This module provides a connection pool for SQLite databases
//! configured with WAL mode for better concurrent performance.

use anyhow::{Context, Result};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::Path;

/// Type alias for the SQLite connection pool
pub type DbPool = Pool<SqliteConnectionManager>;

/// Type alias for a pooled connection
pub type DbConnection = PooledConnection<SqliteConnectionManager>;

/// Configuration for the database pool
#[derive(Debug, Clone)]
pub struct DbPoolConfig {
    /// Maximum number of connections in the pool
    pub max_size: u32,
    /// Whether to enable WAL mode
    pub enable_wal: bool,
    /// Busy timeout in milliseconds
    pub busy_timeout_ms: u32,
}

impl Default for DbPoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10,
            enable_wal: true,
            busy_timeout_ms: 5000,
        }
    }
}

/// Create a new SQLite connection pool
///
/// # Arguments
/// * `db_path` - Path to the SQLite database file
/// * `config` - Pool configuration options
///
/// # Returns
/// A new connection pool configured with WAL mode
pub fn create_pool(db_path: &Path, config: DbPoolConfig) -> Result<DbPool> {
    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create database directory: {:?}", parent))?;
    }

    // Create the connection manager
    let manager = SqliteConnectionManager::file(db_path);

    // Build the pool with custom configuration
    let pool = Pool::builder()
        .max_size(config.max_size)
        .build(manager)
        .context("Failed to create database pool")?;

    // Initialize WAL mode on the first connection
    if config.enable_wal {
        let conn = pool.get().context("Failed to get connection from pool")?;
        configure_wal_mode(&conn, config.busy_timeout_ms)?;
    }

    tracing::info!(
        "Database pool created at {:?} with WAL mode enabled (max_size: {})",
        db_path,
        config.max_size
    );

    Ok(pool)
}

/// Configure SQLite connection for optimal performance
///
/// Enables WAL mode and sets recommended pragmas
fn configure_wal_mode(conn: &Connection, busy_timeout_ms: u32) -> Result<()> {
    conn.execute_batch(&format!(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = {};
        PRAGMA cache_size = -64000;
        PRAGMA temp_store = MEMORY;
        ",
        busy_timeout_ms
    ))
    .context("Failed to configure WAL mode and pragmas")?;

    tracing::debug!("WAL mode and pragmas configured successfully");
    Ok(())
}

/// Get a connection from the pool
///
/// # Arguments
/// * `pool` - The database pool
///
/// # Returns
/// A pooled connection
pub fn get_connection(pool: &DbPool) -> Result<DbConnection> {
    pool.get().context("Failed to get database connection from pool")
}

/// Check if the database file exists
pub fn database_exists(db_path: &Path) -> bool {
    db_path.exists()
}

/// Get the default database path
///
/// Returns the path to the database file in the user's data directory
pub fn default_db_path() -> Result<std::path::PathBuf> {
    let data_dir = directories::ProjectDirs::from("com", "omninova", "omninoval")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|p| p.join(".omninoval"))
                .unwrap_or_else(|| std::path::PathBuf::from(".omninoval"))
        });

    Ok(data_dir.join("data").join("memory.db"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_pool_creates_database() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");

        let pool = create_pool(&db_path, DbPoolConfig::default());
        assert!(pool.is_ok(), "Pool creation should succeed");

        // Verify database file was created
        assert!(db_path.exists(), "Database file should exist");
    }

    #[test]
    fn test_wal_mode_enabled() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test_wal.db");

        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");
        let conn = pool.get().expect("Failed to get connection");

        // Check journal mode is WAL
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("Failed to query journal mode");

        assert_eq!(journal_mode.to_lowercase(), "wal", "Journal mode should be WAL");
    }

    #[test]
    fn test_connection_pool_returns_connections() {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test_pool.db");

        let config = DbPoolConfig {
            max_size: 5,
            ..Default::default()
        };
        let pool = create_pool(&db_path, config).expect("Failed to create pool");

        // Get multiple connections
        let conn1 = pool.get().expect("Failed to get connection 1");
        let conn2 = pool.get().expect("Failed to get connection 2");

        // Connections should be usable
        conn1.execute("CREATE TABLE test1 (id INTEGER)", []).expect("Failed to execute on conn1");
        conn2.execute("CREATE TABLE test2 (id INTEGER)", []).expect("Failed to execute on conn2");
    }

    #[test]
    fn test_default_db_path() {
        let path = default_db_path().expect("Failed to get default path");
        assert!(path.to_string_lossy().contains("omninoval"), "Path should contain omninoval");
        assert!(path.to_string_lossy().contains("memory.db"), "Path should end with memory.db");
    }
}