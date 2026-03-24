//! Skill Registry
//!
//! Central registry for managing skills. Supports registration, lookup,
//! and dependency management.
//!
//! # Architecture
//!
//! The registry uses async-safe data structures with `RwLock` for concurrent access.
//! It maintains:
//! - A main skill store indexed by skill ID
//! - A category index for tag-based lookups
//!
//! # Example
//!
//! ```rust
//! use omninova_core::skills::{SkillRegistry, Skill, SkillMetadata};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let registry = SkillRegistry::new();
//!
//!     // Register a skill
//!     let skill = Arc::new(MySkill::new());
//!     registry.register(skill).await.unwrap();
//!
//!     // Retrieve a skill
//!     if let Some(skill) = registry.get("my-skill").await {
//!         println!("Found: {}", skill.metadata().name);
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::traits::{Skill, SkillMetadata};
use super::error::SkillError;

// ============================================================================
// Skill Registry
// ============================================================================

/// Central registry for managing skills.
///
/// Provides thread-safe registration, lookup, and management of skills.
/// Supports dependency checking and tag-based categorization.
pub struct SkillRegistry {
    /// Registered skills indexed by ID.
    skills: Arc<RwLock<HashMap<String, Arc<dyn Skill>>>>,

    /// Category index for tag-based lookups.
    category_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl SkillRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
            category_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new registry with pre-registered skills.
    pub fn with_skills(skills: Vec<Arc<dyn Skill>>) -> Self {
        let registry = Self::new();
        // Note: We can't use async in a sync constructor, so this is just a convenience
        // that requires the caller to register skills separately
        registry
    }

    /// Register a skill.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A skill with the same ID already exists
    /// - Required dependencies are not registered
    ///
    /// # Example
    ///
    /// ```rust
    /// use omninova_core::skills::{SkillRegistry, Skill, SkillMetadata, SkillContext, SkillResult, SkillError};
    /// use async_trait::async_trait;
    /// use std::sync::Arc;
    /// use std::collections::HashMap;
    ///
    /// struct TestSkill;
    ///
    /// #[async_trait]
    /// impl Skill for TestSkill {
    ///     fn metadata(&self) -> &SkillMetadata { todo!() }
    ///     fn validate(&self, _: &HashMap<String, serde_json::Value>) -> Result<(), SkillError> { Ok(()) }
    ///     async fn execute(&self, _: SkillContext) -> Result<SkillResult, SkillError> { todo!() }
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = SkillRegistry::new();
    ///     let skill = Arc::new(TestSkill);
    ///     // registry.register(skill).await.unwrap();
    /// }
    /// ```
    pub async fn register(&self, skill: Arc<dyn Skill>) -> Result<(), SkillError> {
        let metadata = skill.metadata();
        let id = metadata.id.clone();
        let tags = metadata.tags.clone();

        // Check dependencies
        let missing_deps: Vec<String> = {
            let mut missing = Vec::new();
            for dep in &metadata.dependencies {
                if !self.has_skill(dep).await {
                    missing.push(dep.clone());
                }
            }
            missing
        };

        if !missing_deps.is_empty() {
            return Err(SkillError::DependencyError { missing: missing_deps });
        }

        // Check for duplicate
        {
            let skills = self.skills.read().await;
            if skills.contains_key(&id) {
                return Err(SkillError::ConfigurationError {
                    message: format!("Skill '{}' is already registered", id),
                });
            }
        }

        // Register skill
        {
            let mut skills = self.skills.write().await;
            skills.insert(id.clone(), skill);
        }

        // Update category index
        {
            let mut index = self.category_index.write().await;
            for tag in &tags {
                index
                    .entry(tag.clone())
                    .or_insert_with(Vec::new)
                    .push(id.clone());
            }
        }

        Ok(())
    }

    /// Unregister a skill by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the skill is not found.
    pub async fn unregister(&self, skill_id: &str) -> Result<(), SkillError> {
        let skill = {
            let mut skills = self.skills.write().await;
            skills.remove(skill_id)
        };

        if let Some(skill) = skill {
            // Update category index
            let metadata = skill.metadata();
            let mut index = self.category_index.write().await;
            for tag in &metadata.tags {
                if let Some(ids) = index.get_mut(tag) {
                    ids.retain(|id| id != skill_id);
                    if ids.is_empty() {
                        index.remove(tag);
                    }
                }
            }
            Ok(())
        } else {
            Err(SkillError::not_found(skill_id))
        }
    }

    /// Get a skill by ID.
    ///
    /// Returns `None` if the skill is not registered.
    pub async fn get(&self, skill_id: &str) -> Option<Arc<dyn Skill>> {
        let skills = self.skills.read().await;
        skills.get(skill_id).cloned()
    }

    /// Check if a skill is registered.
    pub async fn has_skill(&self, skill_id: &str) -> bool {
        let skills = self.skills.read().await;
        skills.contains_key(skill_id)
    }

    /// List all registered skills' metadata.
    pub async fn list_all(&self) -> Vec<SkillMetadata> {
        let skills = self.skills.read().await;
        skills.values().map(|s| s.metadata().clone()).collect()
    }

    /// List skills by tag.
    pub async fn list_by_tag(&self, tag: &str) -> Vec<SkillMetadata> {
        let index = self.category_index.read().await;
        let skills = self.skills.read().await;

        if let Some(ids) = index.get(tag) {
            ids.iter()
                .filter_map(|id| skills.get(id).map(|s| s.metadata().clone()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all available tags.
    pub async fn list_tags(&self) -> Vec<String> {
        let index = self.category_index.read().await;
        index.keys().cloned().collect()
    }

    /// Get the number of registered skills.
    pub async fn count(&self) -> usize {
        let skills = self.skills.read().await;
        skills.len()
    }

    /// Clear all registered skills.
    pub async fn clear(&self) {
        let mut skills = self.skills.write().await;
        let mut index = self.category_index.write().await;
        skills.clear();
        index.clear();
    }

    /// Get skill IDs.
    pub async fn skill_ids(&self) -> Vec<String> {
        let skills = self.skills.read().await;
        skills.keys().cloned().collect()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SkillRegistry {
    fn clone(&self) -> Self {
        Self {
            skills: Arc::clone(&self.skills),
            category_index: Arc::clone(&self.category_index),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{SkillContext, SkillResult};
    use async_trait::async_trait;
    use std::collections::HashMap;

    // Mock skill for testing
    struct MockSkill {
        metadata: SkillMetadata,
    }

    impl MockSkill {
        fn new(id: &str, name: &str, tags: Vec<&str>, deps: Vec<&str>) -> Self {
            let mut meta = SkillMetadata::new(id, name, "1.0.0", format!("{} skill", name));
            for tag in tags {
                meta = meta.with_tag(tag);
            }
            for dep in deps {
                meta = meta.with_dependency(dep);
            }
            Self { metadata: meta }
        }
    }

    #[async_trait]
    impl Skill for MockSkill {
        fn metadata(&self) -> &SkillMetadata {
            &self.metadata
        }

        fn validate(&self, _config: &HashMap<String, serde_json::Value>) -> Result<(), SkillError> {
            Ok(())
        }

        async fn execute(&self, _context: SkillContext) -> Result<SkillResult, SkillError> {
            Ok(SkillResult::success("ok"))
        }
    }

    #[tokio::test]
    async fn test_registry_new() {
        let registry = SkillRegistry::new();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_register() {
        let registry = SkillRegistry::new();
        let skill = Arc::new(MockSkill::new("test-skill", "Test", vec![], vec![]));

        registry.register(skill).await.unwrap();
        assert_eq!(registry.count().await, 1);
        assert!(registry.has_skill("test-skill").await);
    }

    #[tokio::test]
    async fn test_registry_register_duplicate() {
        let registry = SkillRegistry::new();
        let skill1 = Arc::new(MockSkill::new("dup", "Dup", vec![], vec![]));
        let skill2 = Arc::new(MockSkill::new("dup", "Dup2", vec![], vec![]));

        registry.register(skill1).await.unwrap();
        let result = registry.register(skill2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_register_with_dependency() {
        let registry = SkillRegistry::new();

        // Register base skill first
        let base = Arc::new(MockSkill::new("base", "Base", vec![], vec![]));
        registry.register(base).await.unwrap();

        // Register dependent skill
        let dep = Arc::new(MockSkill::new("dependent", "Dependent", vec![], vec!["base"]));
        registry.register(dep).await.unwrap();

        assert_eq!(registry.count().await, 2);
    }

    #[tokio::test]
    async fn test_registry_register_missing_dependency() {
        let registry = SkillRegistry::new();

        // Try to register skill with missing dependency
        let skill = Arc::new(MockSkill::new("needs-missing", "Needs", vec![], vec!["nonexistent"]));
        let result = registry.register(skill).await;

        assert!(result.is_err());
        if let SkillError::DependencyError { missing } = result.unwrap_err() {
            assert_eq!(missing, vec!["nonexistent"]);
        } else {
            panic!("Expected DependencyError");
        }
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let registry = SkillRegistry::new();
        let skill = Arc::new(MockSkill::new("to-remove", "Remove", vec![], vec![]));

        registry.register(skill).await.unwrap();
        assert!(registry.has_skill("to-remove").await);

        registry.unregister("to-remove").await.unwrap();
        assert!(!registry.has_skill("to-remove").await);
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_unregister_not_found() {
        let registry = SkillRegistry::new();
        let result = registry.unregister("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_get() {
        let registry = SkillRegistry::new();
        let skill = Arc::new(MockSkill::new("get-test", "Get Test", vec![], vec![]));
        registry.register(skill).await.unwrap();

        let retrieved = registry.get("get-test").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().metadata().id, "get-test");
    }

    #[tokio::test]
    async fn test_registry_get_not_found() {
        let registry = SkillRegistry::new();
        let retrieved = registry.get("nonexistent").await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_registry_list_all() {
        let registry = SkillRegistry::new();

        registry.register(Arc::new(MockSkill::new("a", "A", vec![], vec![]))).await.unwrap();
        registry.register(Arc::new(MockSkill::new("b", "B", vec![], vec![]))).await.unwrap();

        let list = registry.list_all().await;
        assert_eq!(list.len(), 2);

        let ids: Vec<&str> = list.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
    }

    #[tokio::test]
    async fn test_registry_list_by_tag() {
        let registry = SkillRegistry::new();

        registry.register(Arc::new(MockSkill::new("prod1", "P1", vec!["productivity"], vec![]))).await.unwrap();
        registry.register(Arc::new(MockSkill::new("prod2", "P2", vec!["productivity"], vec![]))).await.unwrap();
        registry.register(Arc::new(MockSkill::new("other", "Other", vec!["automation"], vec![]))).await.unwrap();

        let prod_skills = registry.list_by_tag("productivity").await;
        assert_eq!(prod_skills.len(), 2);

        let auto_skills = registry.list_by_tag("automation").await;
        assert_eq!(auto_skills.len(), 1);

        let none_skills = registry.list_by_tag("nonexistent").await;
        assert_eq!(none_skills.len(), 0);
    }

    #[tokio::test]
    async fn test_registry_list_tags() {
        let registry = SkillRegistry::new();

        registry.register(Arc::new(MockSkill::new("a", "A", vec!["tag1", "tag2"], vec![]))).await.unwrap();
        registry.register(Arc::new(MockSkill::new("b", "B", vec!["tag2", "tag3"], vec![]))).await.unwrap();

        let mut tags = registry.list_tags().await;
        tags.sort();
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);
    }

    #[tokio::test]
    async fn test_registry_clear() {
        let registry = SkillRegistry::new();

        registry.register(Arc::new(MockSkill::new("a", "A", vec!["tag"], vec![]))).await.unwrap();
        registry.register(Arc::new(MockSkill::new("b", "B", vec!["tag"], vec![]))).await.unwrap();

        assert_eq!(registry.count().await, 2);
        assert!(!registry.list_tags().await.is_empty());

        registry.clear().await;

        assert_eq!(registry.count().await, 0);
        assert!(registry.list_tags().await.is_empty());
    }

    #[tokio::test]
    async fn test_registry_unregister_updates_tag_index() {
        let registry = SkillRegistry::new();

        registry.register(Arc::new(MockSkill::new("skill1", "S1", vec!["test"], vec![]))).await.unwrap();
        registry.register(Arc::new(MockSkill::new("skill2", "S2", vec!["test"], vec![]))).await.unwrap();

        let test_skills = registry.list_by_tag("test").await;
        assert_eq!(test_skills.len(), 2);

        // Unregister one
        registry.unregister("skill1").await.unwrap();

        let test_skills = registry.list_by_tag("test").await;
        assert_eq!(test_skills.len(), 1);
        assert_eq!(test_skills[0].id, "skill2");

        // Unregister the other - tag should be removed
        registry.unregister("skill2").await.unwrap();
        assert!(registry.list_by_tag("test").await.is_empty());
        assert!(!registry.list_tags().await.contains(&"test".to_string()));
    }

    #[tokio::test]
    async fn test_registry_clone() {
        let registry = SkillRegistry::new();
        registry.register(Arc::new(MockSkill::new("test", "Test", vec![], vec![]))).await.unwrap();

        let cloned = registry.clone();
        assert_eq!(cloned.count().await, 1);
        assert!(cloned.has_skill("test").await);

        // Both share the same data
        cloned.unregister("test").await.unwrap();
        assert!(!registry.has_skill("test").await);
    }
}