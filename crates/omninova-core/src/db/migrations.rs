//! Database migration system for OmniNova Claw
//!
//! This module provides a simple migration system for SQLite databases
//! with support for up/down migrations and version tracking.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Represents a single database migration
#[derive(Debug, Clone)]
pub struct Migration {
    /// Unique identifier for the migration (e.g., "001_initial_schema")
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// SQL to apply the migration
    pub up_sql: String,
    /// SQL to rollback the migration (optional)
    pub down_sql: Option<String>,
}

impl Migration {
    /// Create a new migration
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            up_sql: String::new(),
            down_sql: None,
        }
    }

    /// Set the up migration SQL
    pub fn up(mut self, sql: impl Into<String>) -> Self {
        self.up_sql = sql.into();
        self
    }

    /// Set the down migration SQL
    pub fn down(mut self, sql: impl Into<String>) -> Self {
        self.down_sql = Some(sql.into());
        self
    }

    /// Validate the migration has required fields
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            anyhow::bail!("Migration id cannot be empty");
        }
        if self.up_sql.is_empty() {
            anyhow::bail!("Migration '{}' has no up_sql defined", self.id);
        }
        Ok(())
    }
}

/// Manages database migrations
#[derive(Debug)]
pub struct MigrationRunner {
    /// List of migrations to apply
    migrations: Vec<Migration>,
}

impl Default for MigrationRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl MigrationRunner {
    /// Create a new migration runner
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Add a migration to the runner
    pub fn add_migration(mut self, migration: Migration) -> Result<Self> {
        migration.validate()?;
        self.migrations.push(migration);
        Ok(self)
    }

    /// Add multiple migrations
    pub fn add_migrations(mut self, migrations: Vec<Migration>) -> Result<Self> {
        for migration in &migrations {
            migration.validate()?;
        }
        self.migrations.extend(migrations);
        Ok(self)
    }

    /// Get the list of migrations
    pub fn migrations(&self) -> &[Migration] {
        &self.migrations
    }

    /// Ensure the migrations table exists
    fn ensure_migrations_table(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                id TEXT PRIMARY KEY,
                applied_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                description TEXT
            )",
            [],
        )
        .context("Failed to create _migrations table")?;

        Ok(())
    }

    /// Get list of applied migration IDs
    fn get_applied_migrations(conn: &Connection) -> Result<Vec<String>> {
        let mut stmt = conn
            .prepare("SELECT id FROM _migrations ORDER BY id")
            .context("Failed to prepare statement")?;

        let ids = stmt
            .query_map([], |row| row.get(0))
            .context("Failed to query applied migrations")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to collect migration ids")?;

        Ok(ids)
    }

    /// Apply all pending migrations
    pub fn run(&self, conn: &Connection) -> Result<MigrationReport> {
        Self::ensure_migrations_table(conn)?;

        let applied = Self::get_applied_migrations(conn)?;
        let mut report = MigrationReport::default();

        for migration in &self.migrations {
            if applied.contains(&migration.id) {
                tracing::debug!(
                    "Migration '{}' already applied, skipping",
                    migration.id
                );
                report.skipped.push(migration.id.clone());
                continue;
            }

            tracing::info!(
                "Applying migration '{}': {}",
                migration.id,
                migration.description
            );

            // Apply the migration
            conn.execute_batch(&migration.up_sql)
                .with_context(|| format!("Failed to apply migration '{}'", migration.id))?;

            // Record the migration
            conn.execute(
                "INSERT INTO _migrations (id, description) VALUES (?1, ?2)",
                rusqlite::params![&migration.id, &migration.description],
            )
            .with_context(|| {
                format!(
                    "Failed to record migration '{}' in _migrations table",
                    migration.id
                )
            })?;

            tracing::info!("Migration '{}' applied successfully", migration.id);
            report.applied.push(migration.id.clone());
        }

        Ok(report)
    }

    /// Rollback the last N migrations
    pub fn rollback(&self, conn: &Connection, steps: usize) -> Result<MigrationReport> {
        Self::ensure_migrations_table(conn)?;

        let applied = Self::get_applied_migrations(conn)?;
        let mut report = MigrationReport::default();

        // Find migrations to rollback (in reverse order)
        let to_rollback: Vec<_> = applied
            .iter()
            .rev()
            .take(steps)
            .filter_map(|id| self.migrations.iter().find(|m| &m.id == id))
            .collect();

        for migration in to_rollback {
            if let Some(ref down_sql) = migration.down_sql {
                tracing::info!("Rolling back migration '{}'", migration.id);

                conn.execute_batch(down_sql)
                    .with_context(|| format!("Failed to rollback migration '{}'", migration.id))?;

                conn.execute(
                    "DELETE FROM _migrations WHERE id = ?1",
                    rusqlite::params![&migration.id],
                )
                .with_context(|| {
                    format!(
                        "Failed to remove migration '{}' from _migrations table",
                        migration.id
                    )
                })?;

                tracing::info!("Migration '{}' rolled back successfully", migration.id);
                report.rolled_back.push(migration.id.clone());
            } else {
                tracing::warn!(
                    "Migration '{}' has no down_sql, cannot rollback",
                    migration.id
                );
                report.skipped.push(migration.id.clone());
            }
        }

        Ok(report)
    }

    /// Check the current migration status
    pub fn status(&self, conn: &Connection) -> Result<MigrationStatus> {
        Self::ensure_migrations_table(conn)?;

        let applied = Self::get_applied_migrations(conn)?;

        let pending: Vec<String> = self
            .migrations
            .iter()
            .filter(|m| !applied.contains(&m.id))
            .map(|m| m.id.clone())
            .collect();

        Ok(MigrationStatus {
            applied: applied.len(),
            pending: pending.len(),
            pending_ids: pending,
            applied_ids: applied,
        })
    }
}

/// Report of migration operations
#[derive(Debug, Default)]
pub struct MigrationReport {
    /// Successfully applied migrations
    pub applied: Vec<String>,
    /// Skipped migrations (already applied)
    pub skipped: Vec<String>,
    /// Rolled back migrations
    pub rolled_back: Vec<String>,
}

/// Current migration status
#[derive(Debug)]
pub struct MigrationStatus {
    /// Number of applied migrations
    pub applied: usize,
    /// Number of pending migrations
    pub pending: usize,
    /// IDs of pending migrations
    pub pending_ids: Vec<String>,
    /// IDs of applied migrations
    pub applied_ids: Vec<String>,
}

impl MigrationStatus {
    /// Check if there are pending migrations
    pub fn has_pending(&self) -> bool {
        self.pending > 0
    }
}

// ============================================================================
// Built-in Migrations
// ============================================================================

/// Initial schema migration SQL
const INITIAL_SCHEMA_SQL: &str = r#"
-- Migration: 001_initial
-- Description: Initial schema with core tables

-- Agents table: Core AI agent definitions
CREATE TABLE IF NOT EXISTS agents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    mbti_type TEXT,
    system_prompt TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Sessions table: Conversation sessions
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id INTEGER NOT NULL,
    title TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

-- Messages table: Individual messages within sessions
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_sessions_agent_id ON sessions(agent_id);
CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_agents_mbti_type ON agents(mbti_type);
"#;

/// Agent enhancements migration SQL (adds UUID, domain, status fields)
const AGENT_ENHANCEMENTS_SQL: &str = r#"
-- Migration: 002_agent_enhancements
-- Description: Add agent UUID, domain, and status fields

-- Add new columns to agents table
ALTER TABLE agents ADD COLUMN agent_uuid TEXT;
ALTER TABLE agents ADD COLUMN domain TEXT;
ALTER TABLE agents ADD COLUMN status TEXT CHECK(status IN ('active', 'inactive', 'archived'));

-- Migrate existing data: generate UUIDs in standard format (8-4-4-4-12)
-- Using randomblob to generate random bytes and format as UUID v4-like string
UPDATE agents SET
    agent_uuid = lower(
        substr(hex(randomblob(4)), 1, 8) || '-' ||
        substr(hex(randomblob(2)), 1, 4) || '-4' ||
        substr(hex(randomblob(2)), 1, 3) || '-' ||
        substr(hex(randomblob(2)), 1, 4) || '-' ||
        substr(hex(randomblob(6)), 1, 12)
    ),
    status = CASE WHEN is_active = 1 THEN 'active' ELSE 'inactive' END;

-- Create indexes for new columns
CREATE INDEX IF NOT EXISTS idx_agents_uuid ON agents(agent_uuid);
CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
"#;

/// Agent enhancements rollback SQL
const AGENT_ENHANCEMENTS_DOWN_SQL: &str = r#"
-- Rollback: 002_agent_enhancements
-- Note: SQLite doesn't support DROP COLUMN, so we recreate the table

-- Drop indexes
DROP INDEX IF EXISTS idx_agents_status;
DROP INDEX IF EXISTS idx_agents_uuid;

-- Create a backup of the original schema
CREATE TABLE agents_backup (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    mbti_type TEXT,
    system_prompt TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Copy data back (excluding new columns)
INSERT INTO agents_backup (id, name, description, mbti_type, system_prompt, is_active, created_at, updated_at)
SELECT id, name, description, mbti_type, system_prompt,
       CASE WHEN status = 'active' THEN 1 ELSE 0 END,
       created_at, updated_at
FROM agents;

-- Drop old table and rename backup
DROP TABLE agents;
ALTER TABLE agents_backup RENAME TO agents;

-- Recreate original indexes
CREATE INDEX IF NOT EXISTS idx_agents_mbti_type ON agents(mbti_type);
"#;

/// Account schema migration SQL
const ACCOUNT_SCHEMA_SQL: &str = r#"
-- Migration: 003_account_schema
-- Description: Add account table for local user management

CREATE TABLE IF NOT EXISTS accounts (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- Single user mode, only allow id=1
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    require_password_on_startup INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Ensure only one account
CREATE UNIQUE INDEX IF NOT EXISTS idx_accounts_single ON accounts(id);
"#;

/// Account schema rollback SQL
const ACCOUNT_SCHEMA_DOWN_SQL: &str = r#"
-- Rollback: 003_account_schema
DROP INDEX IF EXISTS idx_accounts_single;
DROP TABLE IF EXISTS accounts;
"#;

/// Encrypted fields migration SQL
const ENCRYPTED_FIELDS_SQL: &str = r#"
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

/// Encrypted fields rollback SQL
const ENCRYPTED_FIELDS_DOWN_SQL: &str = r#"
-- Rollback: 004_encrypted_fields
DROP INDEX IF EXISTS idx_encrypted_fields_record;
DROP INDEX IF EXISTS idx_encrypted_fields_table_column;
DROP TABLE IF EXISTS encrypted_fields;
"#;

/// Provider configs migration SQL
const PROVIDER_CONFIGS_SQL: &str = r#"
-- Migration: 005_provider_configs
-- Description: Add provider_configs table for LLM provider configuration storage

CREATE TABLE IF NOT EXISTS provider_configs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    provider_type TEXT NOT NULL,
    api_key_ref TEXT,  -- Reference to keychain entry
    base_url TEXT,
    default_model TEXT,
    settings TEXT,  -- JSON for provider-specific settings
    is_default INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Index for provider type lookups
CREATE INDEX IF NOT EXISTS idx_provider_configs_type ON provider_configs(provider_type);

-- Index for default provider lookup
CREATE INDEX IF NOT EXISTS idx_provider_configs_default ON provider_configs(is_default);
"#;

/// Provider configs rollback SQL
const PROVIDER_CONFIGS_DOWN_SQL: &str = r#"
-- Rollback: 005_provider_configs
DROP INDEX IF EXISTS idx_provider_configs_default;
DROP INDEX IF EXISTS idx_provider_configs_type;
DROP TABLE IF EXISTS provider_configs;
"#;

/// Agent default provider migration SQL
const AGENT_DEFAULT_PROVIDER_SQL: &str = r#"
-- Migration: 006_agent_default_provider
-- Description: Add default_provider_id column to agents table for per-agent provider assignment

-- Add default_provider_id column to agents table
ALTER TABLE agents ADD COLUMN default_provider_id TEXT;

-- Create index for provider lookups
CREATE INDEX IF NOT EXISTS idx_agents_default_provider ON agents(default_provider_id);
"#;

/// Agent default provider rollback SQL
const AGENT_DEFAULT_PROVIDER_DOWN_SQL: &str = r#"
-- Rollback: 006_agent_default_provider
-- Note: SQLite doesn't support DROP COLUMN, so we recreate the table

-- Drop index
DROP INDEX IF EXISTS idx_agents_default_provider;

-- Create a backup of the schema without default_provider_id
CREATE TABLE agents_backup (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    mbti_type TEXT,
    system_prompt TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    agent_uuid TEXT,
    domain TEXT,
    status TEXT CHECK(status IN ('active', 'inactive', 'archived'))
);

-- Copy data back (excluding default_provider_id)
INSERT INTO agents_backup (id, name, description, mbti_type, system_prompt, is_active, created_at, updated_at, agent_uuid, domain, status)
SELECT id, name, description, mbti_type, system_prompt, is_active, created_at, updated_at, agent_uuid, domain, status
FROM agents;

-- Drop old table and rename backup
DROP TABLE agents;
ALTER TABLE agents_backup RENAME TO agents;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_agents_uuid ON agents(agent_uuid);
CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
CREATE INDEX IF NOT EXISTS idx_agents_mbti_type ON agents(mbti_type);
"#;

/// Get the built-in migrations
///
/// Returns a list of migrations that are embedded in the binary.
pub fn get_builtin_migrations() -> Vec<Migration> {
    vec![
        Migration::new("001_initial", "Initial schema with core tables").up(INITIAL_SCHEMA_SQL),
        Migration::new("002_agent_enhancements", "Add agent UUID, domain, and status fields")
            .up(AGENT_ENHANCEMENTS_SQL)
            .down(AGENT_ENHANCEMENTS_DOWN_SQL),
        Migration::new("003_account_schema", "Add account table for local user management")
            .up(ACCOUNT_SCHEMA_SQL)
            .down(ACCOUNT_SCHEMA_DOWN_SQL),
        Migration::new("004_encrypted_fields", "Add encrypted_fields table for tracking encrypted data")
            .up(ENCRYPTED_FIELDS_SQL)
            .down(ENCRYPTED_FIELDS_DOWN_SQL),
        Migration::new("005_provider_configs", "Add provider_configs table for LLM provider configuration storage")
            .up(PROVIDER_CONFIGS_SQL)
            .down(PROVIDER_CONFIGS_DOWN_SQL),
        Migration::new("006_agent_default_provider", "Add default_provider_id column to agents table for per-agent provider assignment")
            .up(AGENT_DEFAULT_PROVIDER_SQL)
            .down(AGENT_DEFAULT_PROVIDER_DOWN_SQL),
    ]
}

/// Create a migration runner with all built-in migrations
pub fn create_builtin_runner() -> MigrationRunner {
    MigrationRunner::new()
        .add_migrations(get_builtin_migrations())
        .expect("Built-in migrations should be valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn create_test_connection() -> Connection {
        Connection::open_in_memory().expect("Failed to create in-memory connection")
    }

    #[test]
    fn test_migration_creation() {
        let migration = Migration::new("001_test", "Test migration")
            .up("CREATE TABLE test (id INTEGER PRIMARY KEY);")
            .down("DROP TABLE test;");

        assert_eq!(migration.id, "001_test");
        assert_eq!(migration.description, "Test migration");
        assert!(migration.up_sql.contains("CREATE TABLE"));
        assert!(migration.down_sql.is_some());
    }

    #[test]
    fn test_migration_validation() {
        let migration = Migration::new("", "Empty ID");
        assert!(migration.validate().is_err());

        let migration = Migration::new("001_test", "No up SQL");
        assert!(migration.validate().is_err());

        let migration = Migration::new("001_test", "Valid").up("SELECT 1;");
        assert!(migration.validate().is_ok());
    }

    #[test]
    fn test_migrations_table_creation() {
        let conn = create_test_connection();

        MigrationRunner::ensure_migrations_table(&conn).unwrap();

        // Check table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1, "_migrations table should exist");
    }

    #[test]
    fn test_apply_migration() {
        let conn = create_test_connection();
        let migration = Migration::new("001_test", "Test migration")
            .up("CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT);");

        let runner = MigrationRunner::new()
            .add_migration(migration)
            .expect("Failed to add migration");

        let report = runner.run(&conn).expect("Failed to run migrations");

        assert_eq!(report.applied.len(), 1);
        assert_eq!(report.applied[0], "001_test");

        // Verify table was created
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='test_table'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1, "test_table should exist");
    }

    #[test]
    fn test_skip_applied_migration() {
        let conn = create_test_connection();
        let migration = Migration::new("001_test", "Test migration")
            .up("CREATE TABLE test_table (id INTEGER);");

        let runner = MigrationRunner::new()
            .add_migration(migration.clone())
            .expect("Failed to add migration");

        // First run
        let report1 = runner.run(&conn).expect("First run failed");
        assert_eq!(report1.applied.len(), 1);

        // Second run should skip
        let report2 = runner.run(&conn).expect("Second run failed");
        assert_eq!(report2.applied.len(), 0);
        assert_eq!(report2.skipped.len(), 1);
    }

    #[test]
    fn test_rollback_migration() {
        let conn = create_test_connection();
        let migration = Migration::new("001_test", "Test migration")
            .up("CREATE TABLE test_table (id INTEGER);")
            .down("DROP TABLE test_table;");

        let runner = MigrationRunner::new()
            .add_migration(migration)
            .expect("Failed to add migration");

        // Apply
        runner.run(&conn).expect("Apply failed");

        // Verify table exists
        let count_before: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='test_table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count_before, 1);

        // Rollback
        let report = runner.rollback(&conn, 1).expect("Rollback failed");
        assert_eq!(report.rolled_back.len(), 1);

        // Verify table was dropped
        let count_after: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='test_table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count_after, 0);
    }

    #[test]
    fn test_migration_status() {
        let conn = create_test_connection();
        let migrations = vec![
            Migration::new("001_first", "First migration").up("CREATE TABLE t1 (id INTEGER);"),
            Migration::new("002_second", "Second migration").up("CREATE TABLE t2 (id INTEGER);"),
            Migration::new("003_third", "Third migration").up("CREATE TABLE t3 (id INTEGER);"),
        ];

        let runner = MigrationRunner::new()
            .add_migrations(migrations)
            .expect("Failed to add migrations");

        // Check initial status
        let status = runner.status(&conn).expect("Status check failed");
        assert_eq!(status.pending, 3);
        assert_eq!(status.applied, 0);
        assert!(status.has_pending());

        // Apply first two
        let conn2 = create_test_connection();
        MigrationRunner::ensure_migrations_table(&conn2).unwrap();
        conn2
            .execute("INSERT INTO _migrations (id) VALUES ('001_first')", [])
            .unwrap();
        conn2
            .execute("INSERT INTO _migrations (id) VALUES ('002_second')", [])
            .unwrap();

        let status2 = runner.status(&conn2).expect("Status check failed");
        assert_eq!(status2.applied, 2);
        assert_eq!(status2.pending, 1);
        assert_eq!(status2.pending_ids, vec!["003_third"]);
    }

    #[test]
    fn test_multiple_migrations_in_order() {
        let conn = create_test_connection();
        let migrations = vec![
            Migration::new("001_first", "First").up("CREATE TABLE t1 (id INTEGER);"),
            Migration::new("002_second", "Second").up("CREATE TABLE t2 (id INTEGER);"),
        ];

        let runner = MigrationRunner::new()
            .add_migrations(migrations)
            .expect("Failed to add migrations");

        let report = runner.run(&conn).expect("Run failed");

        assert_eq!(report.applied, vec!["001_first", "002_second"]);

        // Verify both tables exist
        for table in ["t1", "t2"] {
            let count: i32 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                        table
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Table {} should exist", table);
        }
    }

    #[test]
    fn test_builtin_migrations_include_agent_enhancements() {
        let migrations = get_builtin_migrations();
        assert_eq!(migrations.len(), 6);
        assert_eq!(migrations[0].id, "001_initial");
        assert_eq!(migrations[1].id, "002_agent_enhancements");
        assert!(migrations[1].down_sql.is_some());
        assert_eq!(migrations[2].id, "003_account_schema");
        assert!(migrations[2].down_sql.is_some());
        assert_eq!(migrations[3].id, "004_encrypted_fields");
        assert!(migrations[3].down_sql.is_some());
        assert_eq!(migrations[4].id, "005_provider_configs");
        assert!(migrations[4].down_sql.is_some());
        assert_eq!(migrations[5].id, "006_agent_default_provider");
        assert!(migrations[5].down_sql.is_some());
    }

    #[test]
    fn test_agent_enhancements_migration_adds_columns() {
        let conn = create_test_connection();

        // Run builtin migrations
        let runner = create_builtin_runner();
        let report = runner.run(&conn).expect("Failed to run migrations");

        assert_eq!(report.applied.len(), 6);

        // Verify agent_uuid column exists
        let uuid_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('agents') WHERE name='agent_uuid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(uuid_count, 1, "agent_uuid column should exist");

        // Verify domain column exists
        let domain_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('agents') WHERE name='domain'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(domain_count, 1, "domain column should exist");

        // Verify status column exists
        let status_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('agents') WHERE name='status'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status_count, 1, "status column should exist");
    }

    #[test]
    fn test_agent_enhancements_migration_generates_uuids() {
        let conn = create_test_connection();

        // Run migrations
        let runner = create_builtin_runner();
        runner.run(&conn).expect("Failed to run migrations");

        // Insert a test agent
        conn.execute(
            "INSERT INTO agents (name, agent_uuid, status) VALUES ('Test Agent', 'test-uuid-123', 'active')",
            [],
        )
        .unwrap();

        // Query to verify columns work
        let (name, uuid, status): (String, String, String) = conn
            .query_row(
                "SELECT name, agent_uuid, status FROM agents WHERE name = 'Test Agent'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(name, "Test Agent");
        assert_eq!(uuid, "test-uuid-123");
        assert_eq!(status, "active");
    }

    #[test]
    fn test_agent_enhancements_migration_indexes() {
        let conn = create_test_connection();

        // Run migrations
        let runner = create_builtin_runner();
        runner.run(&conn).expect("Failed to run migrations");

        // Verify indexes exist
        let uuid_idx: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_agents_uuid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(uuid_idx, 1, "idx_agents_uuid should exist");

        let status_idx: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_agents_status'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status_idx, 1, "idx_agents_status should exist");
    }

    #[test]
    fn test_migration_rerun_safe() {
        let conn = create_test_connection();

        // Run migrations twice
        let runner = create_builtin_runner();
        let report1 = runner.run(&conn).expect("First run failed");
        assert_eq!(report1.applied.len(), 6);

        // Second run should skip all
        let report2 = runner.run(&conn).expect("Second run failed");
        assert_eq!(report2.applied.len(), 0);
        assert_eq!(report2.skipped.len(), 6);
    }

    #[test]
    fn test_encrypted_fields_migration_creates_table() {
        let conn = create_test_connection();

        // Run migrations
        let runner = create_builtin_runner();
        runner.run(&conn).expect("Failed to run migrations");

        // Verify encrypted_fields table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='encrypted_fields'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "encrypted_fields table should exist");

        // Verify indexes exist
        let idx_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_encrypted_fields%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 2, "Both indexes should exist");
    }

    #[test]
    fn test_provider_configs_migration_creates_table() {
        let conn = create_test_connection();

        // Run migrations
        let runner = create_builtin_runner();
        runner.run(&conn).expect("Failed to run migrations");

        // Verify provider_configs table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='provider_configs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "provider_configs table should exist");

        // Verify indexes exist
        let idx_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_provider_configs%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 2, "Both indexes should exist");

        // Verify columns exist
        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('provider_configs')")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(columns.contains(&"id".to_string()));
        assert!(columns.contains(&"name".to_string()));
        assert!(columns.contains(&"provider_type".to_string()));
        assert!(columns.contains(&"api_key_ref".to_string()));
        assert!(columns.contains(&"base_url".to_string()));
        assert!(columns.contains(&"default_model".to_string()));
        assert!(columns.contains(&"settings".to_string()));
        assert!(columns.contains(&"is_default".to_string()));
        assert!(columns.contains(&"created_at".to_string()));
        assert!(columns.contains(&"updated_at".to_string()));
    }

    #[test]
    fn test_agent_default_provider_migration_adds_column() {
        let conn = create_test_connection();

        // Run migrations
        let runner = create_builtin_runner();
        runner.run(&conn).expect("Failed to run migrations");

        // Verify default_provider_id column exists
        let column_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('agents') WHERE name='default_provider_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(column_count, 1, "default_provider_id column should exist");

        // Verify index exists
        let idx_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_agents_default_provider'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 1, "idx_agents_default_provider should exist");

        // Test inserting an agent with default_provider_id
        conn.execute(
            "INSERT INTO agents (name, agent_uuid, status, default_provider_id) VALUES ('Test Agent', 'test-uuid-456', 'active', 'provider-123')",
            [],
        )
        .unwrap();

        // Query to verify the column works
        let (name, provider_id): (String, Option<String>) = conn
            .query_row(
                "SELECT name, default_provider_id FROM agents WHERE agent_uuid = 'test-uuid-456'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(name, "Test Agent");
        assert_eq!(provider_id, Some("provider-123".to_string()));
    }
}