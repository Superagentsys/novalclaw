//! Skill Context Management
//!
//! Provides context and resource access for skill execution.
//!
//! # Architecture
//!
//! The context module provides:
//! - `SkillContext`: Execution context passed to skills
//! - `MemoryAccessor`: Safe access to the memory system
//! - `Permission`: Permission levels for skill actions
//!
//! # Security
//!
//! Skills operate within a sandbox with defined permissions:
//! - Memory access is scoped to the agent and session
//! - File system access requires explicit permission
//! - Network access requires explicit permission

use std::collections::HashMap;
use std::sync::Arc;
use std::fmt;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::memory::manager::{MemoryLayer, MemoryManager, MemoryQuery, UnifiedMemoryEntry};
use crate::providers::traits::ConversationMessage;

use super::error::SkillError;

// ============================================================================
// Permissions
// ============================================================================

/// Permission levels for skill actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read-only access to memory
    MemoryRead,
    /// Write access to memory
    MemoryWrite,
    /// Read files from the file system
    FileRead,
    /// Write files to the file system
    FileWrite,
    /// Execute shell commands
    ExecuteCommand,
    /// Make network requests
    NetworkAccess,
    /// Access external APIs
    ExternalApiAccess,
}

/// Permission set for a skill.
#[derive(Debug, Clone, Default)]
pub struct PermissionSet {
    permissions: Vec<Permission>,
}

impl PermissionSet {
    /// Create an empty permission set.
    pub fn new() -> Self {
        Self { permissions: Vec::new() }
    }

    /// Create a permission set with the given permissions.
    pub fn with(permissions: Vec<Permission>) -> Self {
        Self { permissions }
    }

    /// Create a read-only permission set (memory read only).
    pub fn read_only() -> Self {
        Self { permissions: vec![Permission::MemoryRead] }
    }

    /// Create a full access permission set.
    pub fn full_access() -> Self {
        Self {
            permissions: vec![
                Permission::MemoryRead,
                Permission::MemoryWrite,
                Permission::FileRead,
                Permission::FileWrite,
                Permission::ExecuteCommand,
                Permission::NetworkAccess,
                Permission::ExternalApiAccess,
            ],
        }
    }

    /// Check if a permission is granted.
    pub fn has(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    /// Add a permission.
    pub fn grant(&mut self, permission: Permission) {
        if !self.has(permission) {
            self.permissions.push(permission);
        }
    }

    /// Require a permission, returning an error if not granted.
    pub fn require(&self, permission: Permission) -> Result<(), SkillError> {
        if self.has(permission) {
            Ok(())
        } else {
            Err(SkillError::PermissionError {
                required: format!("{:?}", permission),
            })
        }
    }
}

// ============================================================================
// Memory Accessor
// ============================================================================

/// Safe accessor for the memory system.
///
/// Provides scoped access to the memory system with proper boundaries:
/// - Access is scoped to a specific agent ID
/// - Session scoping is optional
/// - All operations respect permission boundaries
#[derive(Clone)]
pub struct MemoryAccessor {
    /// Reference to the memory manager.
    manager: Arc<RwLock<MemoryManager>>,
    /// Agent ID for scoping.
    agent_id: i64,
    /// Session ID for scoping (optional).
    session_id: Option<i64>,
    /// Permissions for this accessor.
    permissions: PermissionSet,
}

impl fmt::Debug for MemoryAccessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryAccessor")
            .field("agent_id", &self.agent_id)
            .field("session_id", &self.session_id)
            .field("permissions", &self.permissions)
            .finish_non_exhaustive()
    }
}

impl MemoryAccessor {
    /// Create a new memory accessor.
    pub fn new(
        manager: Arc<RwLock<MemoryManager>>,
        agent_id: i64,
        session_id: Option<i64>,
        permissions: PermissionSet,
    ) -> Self {
        Self {
            manager,
            agent_id,
            session_id,
            permissions,
        }
    }

    /// Retrieve relevant memories using semantic search.
    ///
    /// # Errors
    ///
    /// Returns an error if memory read permission is not granted.
    pub async fn retrieve_relevant(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<UnifiedMemoryEntry>, SkillError> {
        self.permissions.require(Permission::MemoryRead)?;

        let manager = self.manager.read().await;
        manager
            .search(query, limit, 0.7)
            .await
            .map_err(|e| SkillError::ExecutionError {
                message: format!("Memory retrieval failed: {}", e),
            })
    }

    /// Retrieve memories from a specific layer.
    ///
    /// # Errors
    ///
    /// Returns an error if memory read permission is not granted.
    pub async fn retrieve_from_layer(
        &self,
        layer: MemoryLayer,
        limit: usize,
    ) -> Result<Vec<UnifiedMemoryEntry>, SkillError> {
        self.permissions.require(Permission::MemoryRead)?;

        let query = MemoryQuery {
            agent_id: self.agent_id,
            session_id: self.session_id,
            layer,
            limit,
            ..Default::default()
        };

        let manager = self.manager.read().await;
        manager
            .retrieve(query)
            .await
            .map(|result| result.entries)
            .map_err(|e| SkillError::ExecutionError {
                message: format!("Memory retrieval from {} failed: {}", layer, e),
            })
    }

    /// Store a memory entry.
    ///
    /// # Errors
    ///
    /// Returns an error if memory write permission is not granted.
    pub async fn store_memory(
        &self,
        content: &str,
        role: &str,
        importance: u8,
    ) -> Result<String, SkillError> {
        self.permissions.require(Permission::MemoryWrite)?;

        let manager = self.manager.read().await;
        let id = manager
            .store(content, role, importance, false, false)
            .await
            .map_err(|e| SkillError::ExecutionError {
                message: format!("Memory storage failed: {}", e),
            })?;
        Ok(id)
    }

    /// Store and index a memory entry (L2 + L3).
    ///
    /// # Errors
    ///
    /// Returns an error if memory write permission is not granted.
    pub async fn store_and_index(
        &self,
        content: &str,
        role: &str,
        importance: u8,
    ) -> Result<String, SkillError> {
        self.permissions.require(Permission::MemoryWrite)?;

        let manager = self.manager.read().await;
        let id = manager
            .store_and_index(content, role, importance)
            .await
            .map_err(|e| SkillError::ExecutionError {
                message: format!("Memory storage and indexing failed: {}", e),
            })?;
        Ok(id)
    }

    /// Get memory statistics.
    pub async fn stats(&self) -> Result<MemoryAccessorStats, SkillError> {
        self.permissions.require(Permission::MemoryRead)?;

        let manager = self.manager.read().await;
        let stats = manager.get_stats().await.map_err(|e| SkillError::ExecutionError {
            message: format!("Failed to get memory stats: {}", e),
        })?;

        Ok(MemoryAccessorStats {
            l1_capacity: stats.l1_capacity,
            l1_used: stats.l1_used,
            l2_total: stats.l2_total,
            l3_total: stats.l3_total,
        })
    }
}

/// Statistics for memory access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAccessorStats {
    pub l1_capacity: usize,
    pub l1_used: usize,
    pub l2_total: usize,
    pub l3_total: usize,
}

// ============================================================================
// Skill Context
// ============================================================================

/// Execution context for skills.
///
/// Provides access to:
/// - Agent and session information
/// - User input and conversation history
/// - Memory system (via accessor)
/// - Configuration and metadata
/// - Permissions for secure operations
#[derive(Clone)]
pub struct SkillContext {
    /// Agent ID executing the skill.
    pub agent_id: String,

    /// Agent name for display purposes.
    pub agent_name: Option<String>,

    /// Session ID if part of a conversation.
    pub session_id: Option<String>,

    /// User input that triggered the skill.
    pub user_input: String,

    /// Conversation history for context.
    pub conversation_history: Vec<ConversationMessage>,

    /// Memory accessor for the skill.
    pub memory: Option<MemoryAccessor>,

    /// Skill configuration parameters.
    pub config: HashMap<String, serde_json::Value>,

    /// Additional metadata.
    pub metadata: HashMap<String, String>,

    /// Permissions for this skill execution.
    pub permissions: PermissionSet,
}

impl fmt::Debug for SkillContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkillContext")
            .field("agent_id", &self.agent_id)
            .field("agent_name", &self.agent_name)
            .field("session_id", &self.session_id)
            .field("user_input", &self.user_input)
            .field("conversation_history", &self.conversation_history.len())
            .field("memory", &self.memory.is_some())
            .field("config", &self.config)
            .field("metadata", &self.metadata)
            .field("permissions", &self.permissions)
            .finish()
    }
}

impl SkillContext {
    /// Create a new skill context with minimal information.
    pub fn new(agent_id: impl Into<String>, user_input: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            agent_name: None,
            session_id: None,
            user_input: user_input.into(),
            conversation_history: Vec::new(),
            memory: None,
            config: HashMap::new(),
            metadata: HashMap::new(),
            permissions: PermissionSet::read_only(),
        }
    }

    /// Set the agent name.
    pub fn with_agent_name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    /// Set the session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the session ID (optional version).
    pub fn with_session_opt(mut self, session_id: Option<&str>) -> Self {
        self.session_id = session_id.map(|s| s.to_string());
        self
    }

    /// Set conversation history.
    pub fn with_history(mut self, history: Vec<ConversationMessage>) -> Self {
        self.conversation_history = history;
        self
    }

    /// Set the memory accessor.
    pub fn with_memory(mut self, memory: MemoryAccessor) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Add a configuration parameter.
    pub fn with_config(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.config.insert(key.into(), value);
        self
    }

    /// Set all configuration parameters.
    pub fn with_config_map(mut self, config: HashMap<String, serde_json::Value>) -> Self {
        self.config = config;
        self
    }

    /// Add a metadata entry.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set permissions.
    pub fn with_permissions(mut self, permissions: PermissionSet) -> Self {
        self.permissions = permissions;
        self
    }

    /// Get a configuration value.
    pub fn get_config<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.config
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get a configuration value with a default.
    pub fn get_config_or<T: for<'de> Deserialize<'de> + Default>(&self, key: &str) -> T {
        self.get_config(key).unwrap_or_default()
    }

    /// Get a config value as a string.
    pub fn config_string(&self, key: &str) -> Option<&str> {
        self.config.get(key).and_then(|v| v.as_str())
    }

    /// Get a config value as an integer.
    pub fn config_int(&self, key: &str) -> Option<i64> {
        self.config.get(key).and_then(|v| v.as_i64())
    }

    /// Get a config value as a boolean.
    pub fn config_bool(&self, key: &str) -> Option<bool> {
        self.config.get(key).and_then(|v| v.as_bool())
    }

    /// Check if a permission is granted.
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.permissions.has(permission)
    }

    /// Require a permission, returning an error if not granted.
    pub fn require_permission(&self, permission: Permission) -> Result<(), SkillError> {
        self.permissions.require(permission)
    }

    /// Get the memory accessor, if available.
    pub fn memory(&self) -> Option<&MemoryAccessor> {
        self.memory.as_ref()
    }
}

// ============================================================================
// Skill Context Builder
// ============================================================================

/// Builder for creating skill contexts.
pub struct SkillContextBuilder {
    agent_id: String,
    agent_name: Option<String>,
    session_id: Option<String>,
    user_input: String,
    conversation_history: Vec<ConversationMessage>,
    memory: Option<MemoryAccessor>,
    config: HashMap<String, serde_json::Value>,
    metadata: HashMap<String, String>,
    permissions: PermissionSet,
}

impl SkillContextBuilder {
    /// Create a new builder.
    pub fn new(agent_id: impl Into<String>, user_input: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            agent_name: None,
            session_id: None,
            user_input: user_input.into(),
            conversation_history: Vec::new(),
            memory: None,
            config: HashMap::new(),
            metadata: HashMap::new(),
            permissions: PermissionSet::read_only(),
        }
    }

    /// Set the agent name.
    pub fn agent_name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    /// Set the session ID.
    pub fn session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set conversation history.
    pub fn history(mut self, history: Vec<ConversationMessage>) -> Self {
        self.conversation_history = history;
        self
    }

    /// Set the memory accessor.
    pub fn memory(mut self, memory: MemoryAccessor) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Add a configuration parameter.
    pub fn config(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.config.insert(key.into(), value);
        self
    }

    /// Set all configuration parameters.
    pub fn config_map(mut self, config: HashMap<String, serde_json::Value>) -> Self {
        self.config = config;
        self
    }

    /// Add a metadata entry.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set permissions.
    pub fn permissions(mut self, permissions: PermissionSet) -> Self {
        self.permissions = permissions;
        self
    }

    /// Build the context.
    pub fn build(self) -> SkillContext {
        SkillContext {
            agent_id: self.agent_id,
            agent_name: self.agent_name,
            session_id: self.session_id,
            user_input: self.user_input,
            conversation_history: self.conversation_history,
            memory: self.memory,
            config: self.config,
            metadata: self.metadata,
            permissions: self.permissions,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_set() {
        let mut perms = PermissionSet::new();
        assert!(!perms.has(Permission::MemoryRead));

        perms.grant(Permission::MemoryRead);
        assert!(perms.has(Permission::MemoryRead));

        assert!(perms.require(Permission::MemoryRead).is_ok());
        assert!(perms.require(Permission::MemoryWrite).is_err());
    }

    #[test]
    fn test_permission_set_read_only() {
        let perms = PermissionSet::read_only();
        assert!(perms.has(Permission::MemoryRead));
        assert!(!perms.has(Permission::MemoryWrite));
    }

    #[test]
    fn test_permission_set_full_access() {
        let perms = PermissionSet::full_access();
        assert!(perms.has(Permission::MemoryRead));
        assert!(perms.has(Permission::MemoryWrite));
        assert!(perms.has(Permission::FileRead));
        assert!(perms.has(Permission::NetworkAccess));
    }

    #[test]
    fn test_skill_context_builder() {
        let context = SkillContextBuilder::new("agent-1", "Hello")
            .agent_name("Test Agent")
            .session("session-1")
            .config("max_results", serde_json::json!(10))
            .build();

        assert_eq!(context.agent_id, "agent-1");
        assert_eq!(context.agent_name, Some("Test Agent".to_string()));
        assert_eq!(context.session_id, Some("session-1".to_string()));
        assert_eq!(context.user_input, "Hello");
        assert_eq!(context.get_config::<i32>("max_results"), Some(10));
    }

    #[test]
    fn test_skill_context_config() {
        let context = SkillContext::new("agent-1", "Test")
            .with_config("key1", serde_json::json!("value1"))
            .with_config("key2", serde_json::json!(42));

        assert_eq!(context.get_config::<String>("key1"), Some("value1".to_string()));
        assert_eq!(context.get_config::<i32>("key2"), Some(42));
        assert_eq!(context.get_config::<String>("nonexistent"), None);
    }

    #[test]
    fn test_skill_context_permissions() {
        let context = SkillContext::new("agent-1", "Test")
            .with_permissions(PermissionSet::with(vec![
                Permission::MemoryRead,
                Permission::FileRead,
            ]));

        assert!(context.has_permission(Permission::MemoryRead));
        assert!(context.has_permission(Permission::FileRead));
        assert!(!context.has_permission(Permission::MemoryWrite));
    }
}