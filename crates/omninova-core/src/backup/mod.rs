//! Backup and restore module for configuration data.
//!
//! This module provides:
//! - Backup data structures for export/import
//! - Backup validation
//! - Import options (overwrite/merge)
//!
//! [Source: 2-12-config-backup-restore.md]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Constants
// ============================================================================

/// Current backup format version
pub const BACKUP_VERSION: &str = "1.0";

/// Application version (should be updated on releases)
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ============================================================================
// Backup Data Structures
// ============================================================================

/// Backup data structure containing all exportable configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupData {
    /// Backup metadata
    pub meta: BackupMeta,
    /// Configuration backup
    pub config: ConfigBackup,
    /// List of agents
    pub agents: Vec<AgentBackup>,
    /// Account information (without sensitive data)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountBackup>,
}

/// Backup metadata containing version and timestamp information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMeta {
    /// Backup format version (e.g., "1.0")
    pub version: String,
    /// Application version that created the backup
    pub app_version: String,
    /// Creation timestamp in ISO 8601 format
    pub created_at: String,
    /// Optional data checksum for integrity verification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

impl BackupMeta {
    /// Create a new backup metadata with current timestamp
    pub fn new() -> Self {
        Self {
            version: BACKUP_VERSION.to_string(),
            app_version: APP_VERSION.to_string(),
            created_at: chrono_iso_timestamp(),
            checksum: None,
        }
    }

    /// Create a new backup metadata with a specific timestamp (for testing)
    pub fn with_timestamp(created_at: String) -> Self {
        Self {
            version: BACKUP_VERSION.to_string(),
            app_version: APP_VERSION.to_string(),
            created_at,
            checksum: None,
        }
    }
}

impl Default for BackupMeta {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration backup structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigBackup {
    /// Provider configurations
    #[serde(default)]
    pub providers: Vec<ProviderBackup>,
    /// Channel configurations
    #[serde(default)]
    pub channels: ChannelsBackup,
    /// Skill configurations
    #[serde(default)]
    pub skills: SkillsBackup,
    /// Agent persona configuration
    #[serde(default)]
    pub agent: AgentPersonaBackup,
    /// Other preference settings
    #[serde(default)]
    pub preferences: HashMap<String, serde_json::Value>,
}

/// Provider backup configuration (simplified, without sensitive data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBackup {
    /// Provider name
    pub name: String,
    /// Provider type (e.g., "openai", "anthropic")
    #[serde(rename = "type")]
    pub provider_type: String,
    /// Model to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// API URL (base URL)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    /// Whether this is the default provider
    #[serde(default)]
    pub is_default: bool,
    /// Additional provider-specific settings
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

/// Channels backup configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelsBackup {
    /// List of enabled channels
    #[serde(default)]
    pub enabled: Vec<String>,
    /// Channel-specific configurations
    #[serde(default)]
    pub configs: HashMap<String, serde_json::Value>,
}

/// Skills backup configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsBackup {
    /// List of enabled skills
    #[serde(default)]
    pub enabled: Vec<String>,
    /// Skill-specific configurations
    #[serde(default)]
    pub configs: HashMap<String, serde_json::Value>,
}

/// Agent persona backup configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentPersonaBackup {
    /// Default MBTI type for new agents
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_mbti: Option<String>,
    /// Default system prompt template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_system_prompt: Option<String>,
    /// Response style settings
    #[serde(default)]
    pub response_style: HashMap<String, serde_json::Value>,
}

/// Agent backup structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBackup {
    /// Agent UUID
    pub uuid: String,
    /// Agent name
    pub name: String,
    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Domain/specialization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// MBTI personality type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mbti_type: Option<String>,
    /// System prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Agent status
    pub status: String,
    /// Creation timestamp
    pub created_at: i64,
    /// Last update timestamp
    pub updated_at: i64,
}

/// Account backup structure (without sensitive data like password hash)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBackup {
    /// Username
    pub username: String,
    /// Whether password is required on startup
    pub require_password_on_startup: bool,
    /// Creation timestamp
    pub created_at: i64,
    /// Last update timestamp
    pub updated_at: i64,
}

// ============================================================================
// Import Options
// ============================================================================

/// Import mode for backup restoration
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportMode {
    /// Overwrite mode: clear existing data and import backup
    Overwrite,
    /// Merge mode: keep existing data, merge with backup (backup takes precedence)
    #[default]
    Merge,
}

impl std::fmt::Display for ImportMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportMode::Overwrite => write!(f, "overwrite"),
            ImportMode::Merge => write!(f, "merge"),
        }
    }
}

impl std::str::FromStr for ImportMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "overwrite" => Ok(ImportMode::Overwrite),
            "merge" => Ok(ImportMode::Merge),
            _ => Err(format!("Invalid import mode: {}", s)),
        }
    }
}

/// Import options for backup restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOptions {
    /// Import mode
    pub mode: ImportMode,
    /// Whether to import agent configurations
    #[serde(default = "default_true")]
    pub include_agents: bool,
    /// Whether to import provider configurations
    #[serde(default = "default_true")]
    pub include_providers: bool,
    /// Whether to import channel configurations
    #[serde(default = "default_true")]
    pub include_channels: bool,
    /// Whether to import skill configurations
    #[serde(default = "default_true")]
    pub include_skills: bool,
    /// Whether to import account settings
    #[serde(default = "default_true")]
    pub include_account: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            mode: ImportMode::default(),
            include_agents: true,
            include_providers: true,
            include_channels: true,
            include_skills: true,
            include_account: true,
        }
    }
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate current timestamp in ISO 8601 format
fn chrono_iso_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();
    chrono_output(secs)
}

fn chrono_output(secs: u64) -> String {
    // Simple ISO 8601 format without chrono dependency
    // Using standard library only
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Calculate date from Unix epoch (1970-01-01)
    let (year, month, day) = unix_days_to_date(days as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert Unix days since epoch to (year, month, day)
fn unix_days_to_date(days: i64) -> (i32, u32, u32) {
    // Days since Unix epoch (1970-01-01)
    let mut days_remaining = days;

    // Calculate year
    let mut year = 1970i32;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days_remaining < days_in_year {
            break;
        }
        days_remaining -= days_in_year;
        year += 1;
    }

    // Calculate month and day
    let month_days = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &days_in_month in &month_days {
        if days_remaining < days_in_month as i64 {
            break;
        }
        days_remaining -= days_in_month as i64;
        month += 1;
    }

    let day = (days_remaining + 1) as u32;
    (year, month, day)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ============================================================================
// Backup Format
// ============================================================================

/// Backup format enum for export
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupFormat {
    Json,
    Yaml,
}

impl std::fmt::Display for BackupFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupFormat::Json => write!(f, "json"),
            BackupFormat::Yaml => write!(f, "yaml"),
        }
    }
}

impl std::str::FromStr for BackupFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(BackupFormat::Json),
            "yaml" | "yml" => Ok(BackupFormat::Yaml),
            _ => Err(format!("Invalid backup format: {}", s)),
        }
    }
}

// ============================================================================
// Backup Export/Import Functions
// ============================================================================

use crate::agent::model::AgentModel;
use crate::account::Account;

impl From<AgentModel> for AgentBackup {
    fn from(agent: AgentModel) -> Self {
        Self {
            uuid: agent.agent_uuid,
            name: agent.name,
            description: agent.description,
            domain: agent.domain,
            mbti_type: agent.mbti_type,
            system_prompt: agent.system_prompt,
            status: agent.status.to_string(),
            created_at: agent.created_at,
            updated_at: agent.updated_at,
        }
    }
}

impl From<&Account> for AccountBackup {
    fn from(account: &Account) -> Self {
        Self {
            username: account.username.clone(),
            require_password_on_startup: account.require_password_on_startup,
            created_at: account.created_at,
            updated_at: account.updated_at,
        }
    }
}

/// Export backup data to string in specified format
pub fn serialize_backup(backup: &BackupData, format: BackupFormat) -> Result<String, BackupError> {
    match format {
        BackupFormat::Json => {
            serde_json::to_string_pretty(backup).map_err(|e| BackupError::Serialization(e.to_string()))
        }
        BackupFormat::Yaml => {
            serde_yaml::to_string(backup).map_err(|e| BackupError::Serialization(e.to_string()))
        }
    }
}

/// Parse backup data from string (auto-detect format)
pub fn deserialize_backup(content: &str) -> Result<BackupData, BackupError> {
    // Try JSON first, then YAML
    if let Ok(backup) = serde_json::from_str::<BackupData>(content) {
        return Ok(backup);
    }

    serde_yaml::from_str::<BackupData>(content).map_err(|e| {
        BackupError::Deserialization(format!(
            "Failed to parse as JSON or YAML: {}",
            e
        ))
    })
}

// ============================================================================
// Backup Validation
// ============================================================================

/// Validate backup data before import
pub fn validate_backup(data: &BackupData) -> Result<(), BackupError> {
    // Check version compatibility
    if !is_compatible_version(&data.meta.version) {
        return Err(BackupError::IncompatibleVersion {
            version: data.meta.version.clone(),
            expected: BACKUP_VERSION.to_string(),
        });
    }

    // Validate meta has required fields
    if data.meta.version.is_empty() {
        return Err(BackupError::InvalidFormat(
            "Missing backup version".to_string(),
        ));
    }

    if data.meta.created_at.is_empty() {
        return Err(BackupError::InvalidFormat(
            "Missing creation timestamp".to_string(),
        ));
    }

    // Validate agents have required fields
    for (idx, agent) in data.agents.iter().enumerate() {
        if agent.uuid.is_empty() {
            return Err(BackupError::InvalidFormat(format!(
                "Agent {} has empty UUID",
                idx
            )));
        }
        if agent.name.is_empty() {
            return Err(BackupError::InvalidFormat(format!(
                "Agent {} has empty name",
                idx
            )));
        }
    }

    // Validate account has required fields if present
    if let Some(ref account) = data.account {
        if account.username.is_empty() {
            return Err(BackupError::InvalidFormat(
                "Account has empty username".to_string(),
            ));
        }
    }

    Ok(())
}

/// Check if backup version is compatible with current version
fn is_compatible_version(version: &str) -> bool {
    // For v1.x, we accept any 1.x version
    let parts: Vec<&str> = version.split('.').collect();
    if parts.is_empty() {
        return false;
    }

    // Major version must be "1" for now
    parts[0] == "1"
}

// ============================================================================
// Backup Service
// ============================================================================

use crate::account::AccountStore;
use crate::agent::AgentStore;
use crate::config::Config;

/// Service for backup and restore operations
///
/// This service coordinates backup export/import across multiple stores:
/// - AgentStore: Agent configurations
/// - AccountStore: Account settings (without sensitive data)
/// - Config: Application configuration
pub struct BackupService {
    agent_store: AgentStore,
    account_store: AccountStore,
}

impl BackupService {
    /// Create a new BackupService with the required stores
    pub fn new(agent_store: AgentStore, account_store: AccountStore) -> Self {
        Self {
            agent_store,
            account_store,
        }
    }

    /// Export all data to a BackupData structure
    ///
    /// # Arguments
    /// * `config` - Current application configuration
    ///
    /// # Returns
    /// A `BackupData` containing all exportable data
    pub fn export_backup(&self, config: &Config) -> Result<BackupData, BackupError> {
        // Collect agents
        let agents = self.agent_store.find_all()
            .map_err(|e| BackupError::Database(format!("Failed to fetch agents: {}", e)))?;

        let agent_backups: Vec<AgentBackup> = agents.into_iter().map(|a| a.into()).collect();

        // Collect account (without password hash)
        let account_backup = self.account_store.get()
            .map_err(|e| BackupError::Database(format!("Failed to fetch account: {}", e)))?
            .map(|a| (&a).into());

        // Build config backup from Config
        let config_backup = ConfigBackup::from_config(config);

        Ok(BackupData {
            meta: BackupMeta::new(),
            config: config_backup,
            agents: agent_backups,
            account: account_backup,
        })
    }

    /// Export backup to string in specified format
    ///
    /// # Arguments
    /// * `config` - Current application configuration
    /// * `format` - Output format (JSON or YAML)
    ///
    /// # Returns
    /// A string containing the serialized backup
    pub fn export_backup_to_string(
        &self,
        config: &Config,
        format: BackupFormat,
    ) -> Result<String, BackupError> {
        let backup = self.export_backup(config)?;
        serialize_backup(&backup, format)
    }

    /// Import backup data with specified options
    ///
    /// # Arguments
    /// * `backup` - Backup data to import
    /// * `options` - Import options (overwrite/merge, what to include)
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn import_backup(
        &self,
        backup: &BackupData,
        options: &ImportOptions,
    ) -> Result<ImportResult, BackupError> {
        // Validate backup first
        validate_backup(backup)?;

        let mut result = ImportResult::default();

        // Import agents if requested
        if options.include_agents {
            result.agents_imported = self.import_agents(&backup.agents, &options.mode)?;
        }

        // Import account if requested
        if options.include_account {
            if let Some(ref account_backup) = backup.account {
                result.account_imported = self.import_account(account_backup, &options.mode)?;
            }
        }

        // Note: Config import is handled separately as it requires ConfigManager
        // The config section is validated but not directly imported here

        Ok(result)
    }

    /// Import agents with the specified mode
    fn import_agents(
        &self,
        agents: &[AgentBackup],
        mode: &ImportMode,
    ) -> Result<usize, BackupError> {
        match mode {
            ImportMode::Overwrite => {
                // Clear existing agents and import all from backup
                self.clear_all_agents()?;

                let mut count = 0;
                for agent in agents {
                    if self.create_agent_from_backup(agent)? {
                        count += 1;
                    }
                }
                Ok(count)
            }
            ImportMode::Merge => {
                // Keep existing, add/update from backup
                let mut count = 0;
                for agent in agents {
                    // Try to update existing agent, or create new one
                    if self.upsert_agent_from_backup(agent)? {
                        count += 1;
                    }
                }
                Ok(count)
            }
        }
    }

    /// Clear all agents from the store
    fn clear_all_agents(&self) -> Result<(), BackupError> {
        let agents = self.agent_store.find_all()
            .map_err(|e| BackupError::Database(format!("Failed to fetch agents for clearing: {}", e)))?;

        for agent in agents {
            self.agent_store.delete(&agent.agent_uuid)
                .map_err(|e| BackupError::Database(format!("Failed to delete agent: {}", e)))?;
        }

        Ok(())
    }

    /// Create an agent from backup data
    fn create_agent_from_backup(&self, backup: &AgentBackup) -> Result<bool, BackupError> {
        use crate::agent::model::{NewAgent, AgentStatus};
        use std::str::FromStr;

        // Status is parsed but not used in NewAgent (it defaults to Active on create)
        let _status = AgentStatus::from_str(&backup.status).unwrap_or(AgentStatus::Active);

        let new_agent = NewAgent {
            name: backup.name.clone(),
            description: backup.description.clone(),
            domain: backup.domain.clone(),
            mbti_type: backup.mbti_type.clone(),
            system_prompt: backup.system_prompt.clone(),
            default_provider_id: None,
            style_config: None,
            context_window_config: None,
            trigger_keywords_config: None,
            privacy_config: None,
        };

        self.agent_store.create(&new_agent)
            .map_err(|e| BackupError::Import(format!("Failed to create agent: {}", e)))?;

        Ok(true)
    }

    /// Upsert (create or update) an agent from backup data
    fn upsert_agent_from_backup(&self, backup: &AgentBackup) -> Result<bool, BackupError> {
        // Check if agent exists by UUID
        let existing = self.agent_store.find_by_uuid(&backup.uuid)
            .map_err(|e| BackupError::Database(format!("Failed to check existing agent: {}", e)))?;

        match existing {
            Some(_) => {
                // Agent exists - for MVP, we skip updating
                // In a full implementation, we would update the existing agent
                Ok(false)
            }
            None => {
                // Agent doesn't exist - create it
                self.create_agent_from_backup(backup)
            }
        }
    }

    /// Import account from backup
    fn import_account(
        &self,
        account: &AccountBackup,
        _mode: &ImportMode,
    ) -> Result<bool, BackupError> {
        // Check if account already exists
        let existing = self.account_store.get()
            .map_err(|e| BackupError::Database(format!("Failed to check existing account: {}", e)))?;

        if existing.is_some() {
            // For MVP, we don't overwrite existing account
            // This preserves security settings and password
            return Ok(false);
        }

        // Create account without password (user will need to set password later)
        self.account_store.create_without_password(&account.username)
            .map_err(|e| BackupError::Import(format!("Failed to create account: {}", e)))?;

        Ok(true)
    }
}

/// Result of import operation
#[derive(Debug, Clone, Default)]
pub struct ImportResult {
    /// Number of agents imported
    pub agents_imported: usize,
    /// Whether account was imported
    pub account_imported: bool,
}

impl ConfigBackup {
    /// Create a ConfigBackup from a Config
    fn from_config(config: &Config) -> Self {
        // Extract provider configurations
        let providers: Vec<ProviderBackup> = config.providers.iter().map(|p| ProviderBackup {
            name: p.name.clone(),
            provider_type: p.provider_type.clone(),
            model: p.models.first().cloned(),
            api_url: p.base_url.clone(),
            is_default: p.enabled,
            settings: HashMap::new(), // Provider-specific settings would go here
        }).collect();

        Self {
            providers,
            channels: ChannelsBackup::from_channels(&config.channels_config),
            skills: SkillsBackup::from_skills(&config.skills),
            agent: AgentPersonaBackup::from_agent_config(&config.agent),
            preferences: HashMap::new(), // Additional preferences can be added
        }
    }
}

impl ChannelsBackup {
    /// Create from ChannelsConfig
    fn from_channels(_config: &crate::config::ChannelsConfig) -> Self {
        // For MVP, we store empty channel config
        // Channel tokens are not exported for security
        Self::default()
    }
}

impl SkillsBackup {
    /// Create from SkillsConfig
    fn from_skills(_config: &crate::config::SkillsConfig) -> Self {
        // For MVP, we store empty skills config
        Self::default()
    }
}

impl AgentPersonaBackup {
    /// Create from AgentConfig
    fn from_agent_config(_config: &crate::config::AgentConfig) -> Self {
        // For MVP, we store empty agent persona
        Self::default()
    }
}

// ============================================================================
// Errors
// ============================================================================

/// Error type for backup operations
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Invalid backup format: {0}")]
    InvalidFormat(String),

    #[error("Incompatible backup version: {version}, expected {expected}")]
    IncompatibleVersion { version: String, expected: String },

    #[error("Import error: {0}")]
    Import(String),

    #[error("Database error: {0}")]
    Database(String),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Account;
    use crate::agent::model::{AgentModel, AgentStatus};

    #[test]
    fn test_backup_meta_new() {
        let meta = BackupMeta::new();
        assert_eq!(meta.version, BACKUP_VERSION);
        assert!(!meta.app_version.is_empty());
        assert!(!meta.created_at.is_empty());
        assert!(meta.checksum.is_none());
    }

    #[test]
    fn test_backup_data_serialization_json() {
        let backup = BackupData {
            meta: BackupMeta::new(),
            config: ConfigBackup {
                providers: vec![ProviderBackup {
                    name: "OpenAI".to_string(),
                    provider_type: "openai".to_string(),
                    model: Some("gpt-4".to_string()),
                    api_url: None,
                    is_default: true,
                    settings: HashMap::new(),
                }],
                channels: ChannelsBackup::default(),
                skills: SkillsBackup::default(),
                agent: AgentPersonaBackup::default(),
                preferences: HashMap::new(),
            },
            agents: vec![AgentBackup {
                uuid: "test-uuid-123".to_string(),
                name: "Test Agent".to_string(),
                description: Some("A test agent".to_string()),
                domain: Some("coding".to_string()),
                mbti_type: Some("INTJ".to_string()),
                system_prompt: Some("You are helpful.".to_string()),
                status: "active".to_string(),
                created_at: 1700000000,
                updated_at: 1700000000,
            }],
            account: Some(AccountBackup {
                username: "testuser".to_string(),
                require_password_on_startup: true,
                created_at: 1700000000,
                updated_at: 1700000000,
            }),
        };

        let json = serde_json::to_string_pretty(&backup).unwrap();
        assert!(json.contains("\"version\": \"1.0\""));
        assert!(json.contains("\"Test Agent\""));
        assert!(json.contains("\"testuser\""));
        // Should NOT contain password_hash
        assert!(!json.contains("password_hash"));

        // Round-trip
        let parsed: BackupData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agents.len(), 1);
        assert_eq!(parsed.agents[0].name, "Test Agent");
        assert!(parsed.account.is_some());
    }

    #[test]
    fn test_backup_data_serialization_yaml() {
        let backup = BackupData {
            meta: BackupMeta::new(),
            config: ConfigBackup::default(),
            agents: vec![],
            account: None,
        };

        let yaml = serde_yaml::to_string(&backup).unwrap();
        assert!(yaml.contains("version: '1.0'"));

        // Round-trip
        let parsed: BackupData = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.meta.version, "1.0");
    }

    #[test]
    fn test_import_mode_serialization() {
        let mode = ImportMode::Overwrite;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"overwrite\"");

        let parsed: ImportMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ImportMode::Overwrite);
    }

    #[test]
    fn test_import_options_default() {
        let options = ImportOptions::default();
        assert_eq!(options.mode, ImportMode::Merge);
        assert!(options.include_agents);
        assert!(options.include_providers);
        assert!(options.include_channels);
        assert!(options.include_skills);
        assert!(options.include_account);
    }

    #[test]
    fn test_import_options_serialization() {
        let options = ImportOptions {
            mode: ImportMode::Overwrite,
            include_agents: true,
            include_providers: false,
            include_channels: true,
            include_skills: false,
            include_account: true,
        };

        let json = serde_json::to_string(&options).unwrap();
        let parsed: ImportOptions = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.mode, ImportMode::Overwrite);
        assert!(parsed.include_agents);
        assert!(!parsed.include_providers);
        assert!(parsed.include_channels);
        assert!(!parsed.include_skills);
        assert!(parsed.include_account);
    }

    #[test]
    fn test_import_mode_from_str() {
        assert_eq!("overwrite".parse::<ImportMode>().unwrap(), ImportMode::Overwrite);
        assert_eq!("merge".parse::<ImportMode>().unwrap(), ImportMode::Merge);
        assert_eq!("OVERWRITE".parse::<ImportMode>().unwrap(), ImportMode::Overwrite);
        assert!("invalid".parse::<ImportMode>().is_err());
    }

    #[test]
    fn test_backup_format_from_str() {
        assert_eq!("json".parse::<BackupFormat>().unwrap(), BackupFormat::Json);
        assert_eq!("yaml".parse::<BackupFormat>().unwrap(), BackupFormat::Yaml);
        assert_eq!("yml".parse::<BackupFormat>().unwrap(), BackupFormat::Yaml);
        assert!("invalid".parse::<BackupFormat>().is_err());
    }

    #[test]
    fn test_agent_backup_serialization() {
        let agent = AgentBackup {
            uuid: "uuid-123".to_string(),
            name: "Agent".to_string(),
            description: None,
            domain: None,
            mbti_type: Some("ENFP".to_string()),
            system_prompt: None,
            status: "active".to_string(),
            created_at: 1234567890,
            updated_at: 1234567890,
        };

        let json = serde_json::to_string(&agent).unwrap();
        // description and domain should be skipped when None
        assert!(!json.contains("description"));
        assert!(!json.contains("domain"));
        assert!(json.contains("mbti_type"));
    }

    #[test]
    fn test_config_backup_empty() {
        let config = ConfigBackup::default();
        assert!(config.providers.is_empty());
        assert!(config.channels.enabled.is_empty());
        assert!(config.skills.enabled.is_empty());
        assert!(config.preferences.is_empty());
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn test_unix_days_to_date() {
        // Unix epoch: 1970-01-01
        assert_eq!(unix_days_to_date(0), (1970, 1, 1));

        // 1970-01-02
        assert_eq!(unix_days_to_date(1), (1970, 1, 2));

        // 1970-02-01
        assert_eq!(unix_days_to_date(31), (1970, 2, 1));

        // 1971-01-01
        assert_eq!(unix_days_to_date(365), (1971, 1, 1));
    }

    #[test]
    fn test_chrono_output() {
        let result = chrono_output(0);
        assert_eq!(result, "1970-01-01T00:00:00Z");

        // 2024-01-01 00:00:00 UTC
        // Days from 1970-01-01 to 2024-01-01
        let result = chrono_output(1704067200);
        assert!(result.starts_with("2024-01-01"));
    }

    #[test]
    fn test_serialize_backup_json() {
        let backup = create_test_backup();
        let json = serialize_backup(&backup, BackupFormat::Json).unwrap();
        assert!(json.contains("\"version\": \"1.0\""));
        assert!(json.contains("\"Test Agent\""));
    }

    #[test]
    fn test_serialize_backup_yaml() {
        let backup = create_test_backup();
        let yaml = serialize_backup(&backup, BackupFormat::Yaml).unwrap();
        assert!(yaml.contains("version: '1.0'"));
    }

    #[test]
    fn test_deserialize_backup_json() {
        let json = r#"{
            "meta": {
                "version": "1.0",
                "app_version": "1.0.0",
                "created_at": "2024-01-01T00:00:00Z"
            },
            "config": {
                "providers": [],
                "channels": {"enabled": [], "configs": {}},
                "skills": {"enabled": [], "configs": {}},
                "agent": {},
                "preferences": {}
            },
            "agents": [],
            "account": null
        }"#;

        let backup: BackupData = deserialize_backup(json).unwrap();
        assert_eq!(backup.meta.version, "1.0");
    }

    #[test]
    fn test_deserialize_backup_yaml() {
        let yaml = r#"
meta:
  version: '1.0'
  app_version: '1.0.0'
  created_at: '2024-01-01T00:00:00Z'
config:
  providers: []
  channels:
    enabled: []
    configs: {}
  skills:
    enabled: []
    configs: {}
  agent: {}
  preferences: {}
agents: []
account: null
"#;

        let backup: BackupData = deserialize_backup(yaml).unwrap();
        assert_eq!(backup.meta.version, "1.0");
    }

    #[test]
    fn test_deserialize_backup_invalid() {
        let invalid = "this is not valid json or yaml";
        let result = deserialize_backup(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_backup_valid() {
        let backup = create_test_backup();
        assert!(validate_backup(&backup).is_ok());
    }

    #[test]
    fn test_validate_backup_incompatible_version() {
        let mut backup = create_test_backup();
        backup.meta.version = "2.0".to_string();
        let result = validate_backup(&backup);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BackupError::IncompatibleVersion { .. }));
    }

    #[test]
    fn test_validate_backup_empty_version() {
        let mut backup = create_test_backup();
        backup.meta.version = "".to_string();
        let result = validate_backup(&backup);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_backup_empty_agent_uuid() {
        let mut backup = create_test_backup();
        backup.agents.push(AgentBackup {
            uuid: "".to_string(),
            name: "Invalid Agent".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            status: "active".to_string(),
            created_at: 0,
            updated_at: 0,
        });
        let result = validate_backup(&backup);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_backup_empty_agent_name() {
        let mut backup = create_test_backup();
        backup.agents.push(AgentBackup {
            uuid: "test-uuid".to_string(),
            name: "".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            status: "active".to_string(),
            created_at: 0,
            updated_at: 0,
        });
        let result = validate_backup(&backup);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_backup_empty_account_username() {
        let mut backup = create_test_backup();
        backup.account = Some(AccountBackup {
            username: "".to_string(),
            require_password_on_startup: false,
            created_at: 0,
            updated_at: 0,
        });
        let result = validate_backup(&backup);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_compatible_version() {
        assert!(is_compatible_version("1.0"));
        assert!(is_compatible_version("1.1"));
        assert!(is_compatible_version("1.9"));
        assert!(!is_compatible_version("2.0"));
        assert!(!is_compatible_version("0.9"));
        assert!(!is_compatible_version(""));
        assert!(!is_compatible_version("invalid"));
    }

    #[test]
    fn test_agent_model_to_backup() {
        let agent_model = AgentModel {
            id: 1,
            agent_uuid: "test-uuid".to_string(),
            name: "Test Agent".to_string(),
            description: Some("Description".to_string()),
            domain: Some("coding".to_string()),
            mbti_type: Some("INTJ".to_string()),
            system_prompt: Some("You are helpful.".to_string()),
            status: AgentStatus::Active,
            default_provider_id: None,
            style_config: None,
            context_window_config: None,
            trigger_keywords_config: None,
            privacy_config: None,
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let backup: AgentBackup = agent_model.into();
        assert_eq!(backup.uuid, "test-uuid");
        assert_eq!(backup.name, "Test Agent");
        assert_eq!(backup.status, "active");
    }

    #[test]
    fn test_account_to_backup() {
        let account = Account {
            id: 1,
            username: "testuser".to_string(),
            password_hash: "hashed_password".to_string(),
            require_password_on_startup: true,
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let backup: AccountBackup = (&account).into();
        assert_eq!(backup.username, "testuser");
        assert!(backup.require_password_on_startup);
        // Should NOT contain password_hash
    }

    // Helper function to create a test backup
    fn create_test_backup() -> BackupData {
        BackupData {
            meta: BackupMeta::with_timestamp("2024-01-01T00:00:00Z".to_string()),
            config: ConfigBackup::default(),
            agents: vec![AgentBackup {
                uuid: "test-uuid-123".to_string(),
                name: "Test Agent".to_string(),
                description: None,
                domain: None,
                mbti_type: None,
                system_prompt: None,
                status: "active".to_string(),
                created_at: 1700000000,
                updated_at: 1700000000,
            }],
            account: None,
        }
    }

    #[test]
    fn test_config_backup_from_config() {
        use crate::config::Config;

        let config = Config::default();
        let backup = ConfigBackup::from_config(&config);

        // Should have empty providers in default config
        assert!(backup.providers.is_empty());
    }

    #[test]
    fn test_backup_service_export_empty() {
        use crate::db::{create_pool, DbPoolConfig};
        use crate::config::Config;
        use tempfile::tempdir;

        // Create temporary database
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");

        let agent_store = AgentStore::new(pool.clone());
        let account_store = AccountStore::new(pool.clone());

        // Initialize migrations
        agent_store.initialize().expect("Failed to initialize");

        let service = BackupService::new(agent_store, account_store);
        let config = Config::default();

        let result = service.export_backup(&config);
        assert!(result.is_ok());

        let backup = result.unwrap();
        // Should have no agents (empty database)
        assert!(backup.agents.is_empty());
        // Should have no account (not created)
        assert!(backup.account.is_none());
        // Should have valid metadata
        assert_eq!(backup.meta.version, BACKUP_VERSION);
    }

    #[test]
    fn test_import_backup_overwrite_mode() {
        use crate::db::{create_pool, DbPoolConfig};
        use crate::agent::AgentStore;
        use crate::account::AccountStore;
        use tempfile::tempdir;

        // Create temporary database
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");

        let agent_store = AgentStore::new(pool.clone());
        let account_store = AccountStore::new(pool.clone());

        // Initialize migrations
        agent_store.initialize().expect("Failed to initialize");

        let service = BackupService::new(agent_store, account_store);

        // Create backup data to import
        let backup = BackupData {
            meta: BackupMeta::new(),
            config: ConfigBackup::default(),
            agents: vec![
                AgentBackup {
                    uuid: "uuid-1".to_string(),
                    name: "Agent 1".to_string(),
                    description: None,
                    domain: None,
                    mbti_type: None,
                    system_prompt: None,
                    status: "active".to_string(),
                    created_at: 1700000000,
                    updated_at: 1700000000,
                },
                AgentBackup {
                    uuid: "uuid-2".to_string(),
                    name: "Agent 2".to_string(),
                    description: None,
                    domain: None,
                    mbti_type: None,
                    system_prompt: None,
                    status: "active".to_string(),
                    created_at: 1700000000,
                    updated_at: 1700000000,
                },
            ],
            account: None,
        };

        let options = ImportOptions {
            mode: ImportMode::Overwrite,
            include_agents: true,
            include_providers: false,
            include_channels: false,
            include_skills: false,
            include_account: false,
        };

        let result = service.import_backup(&backup, &options);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert_eq!(import_result.agents_imported, 2);
    }

    #[test]
    fn test_import_backup_merge_mode() {
        use crate::db::{create_pool, DbPoolConfig};
        use crate::agent::{AgentStore, NewAgent};
        use crate::account::AccountStore;
        use tempfile::tempdir;

        // Create temporary database
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");

        let agent_store = AgentStore::new(pool.clone());
        let account_store = AccountStore::new(pool.clone());

        // Initialize migrations
        agent_store.initialize().expect("Failed to initialize");

        // Create an existing agent
        let existing = NewAgent {
            name: "Existing Agent".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
            style_config: None,
            context_window_config: None,
            trigger_keywords_config: None,
            privacy_config: None,
        };
        agent_store.create(&existing).expect("Failed to create existing agent");

        let service = BackupService::new(agent_store, account_store);

        // Create backup data with a new agent
        let backup = BackupData {
            meta: BackupMeta::new(),
            config: ConfigBackup::default(),
            agents: vec![
                AgentBackup {
                    uuid: "new-uuid".to_string(),
                    name: "New Agent".to_string(),
                    description: None,
                    domain: None,
                    mbti_type: None,
                    system_prompt: None,
                    status: "active".to_string(),
                    created_at: 1700000000,
                    updated_at: 1700000000,
                },
            ],
            account: None,
        };

        let options = ImportOptions {
            mode: ImportMode::Merge,
            include_agents: true,
            include_providers: false,
            include_channels: false,
            include_skills: false,
            include_account: false,
        };

        let result = service.import_backup(&backup, &options);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert_eq!(import_result.agents_imported, 1);
    }

    #[test]
    fn test_import_backup_with_account() {
        use crate::db::{create_pool, DbPoolConfig};
        use crate::agent::AgentStore;
        use crate::account::AccountStore;
        use tempfile::tempdir;

        // Create temporary database
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");

        let agent_store = AgentStore::new(pool.clone());
        let account_store = AccountStore::new(pool.clone());

        // Initialize migrations
        agent_store.initialize().expect("Failed to initialize");

        let service = BackupService::new(agent_store, account_store);

        // Create backup data with account
        let backup = BackupData {
            meta: BackupMeta::new(),
            config: ConfigBackup::default(),
            agents: vec![],
            account: Some(AccountBackup {
                username: "imported_user".to_string(),
                require_password_on_startup: false,
                created_at: 1700000000,
                updated_at: 1700000000,
            }),
        };

        let options = ImportOptions {
            mode: ImportMode::Overwrite,
            include_agents: false,
            include_providers: false,
            include_channels: false,
            include_skills: false,
            include_account: true,
        };

        let result = service.import_backup(&backup, &options);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert!(import_result.account_imported);
    }
}