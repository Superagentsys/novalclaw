//! Skills System
//!
//! This module provides an extensible skill system for AI agents.
//!
//! # Architecture
//!
//! The skills system consists of:
//! - `traits`: Core Skill trait and data structures
//! - `registry`: SkillRegistry for managing skills
//! - `openclaw`: OpenClaw skill format compatibility
//! - `context`: Skill context and memory accessor
//! - File-based skills: Legacy skill loading from SKILL.md files
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
//!     // Register and use skills
//! }
//! ```

// Core skill trait and types
pub mod traits;
pub mod registry;
pub mod openclaw;
pub mod context;
pub mod error;
pub mod executor;

// Re-export main types for convenience
pub use traits::{Skill, SkillMetadata, SkillResult};
pub use registry::SkillRegistry;
pub use openclaw::{OpenClawSkillAdapter, OpenClawSkillDefinition, OpenClawParameter};
pub use context::{SkillContext, MemoryAccessor, Permission, PermissionSet, SkillContextBuilder};
pub use error::SkillError;
pub use executor::{SkillExecutor, SkillExecutorConfig, ExecutionLog, CacheStats};

// ============================================================================
// File-based Skills (Legacy)
// ============================================================================

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Context, Result};
use tracing::warn;

/// File-based skill metadata (legacy format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSkillMetadata {
    pub name: String,
    pub description: String,
    pub homepage: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// File-based skill loaded from SKILL.md files
#[derive(Debug, Clone)]
pub struct FileSkill {
    pub metadata: FileSkillMetadata,
    pub content: String,
    pub path: PathBuf,
}

impl FileSkill {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("Failed to read skill file: {:?}", path))?;

        let parts: Vec<&str> = raw.splitn(3, "---").collect();
        if parts.len() < 3 {
             let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
             return Ok(FileSkill {
                 metadata: FileSkillMetadata {
                     name: name.clone(),
                     description: "No description provided.".to_string(),
                     homepage: None,
                     metadata: serde_json::Value::Null,
                 },
                 content: raw,
                 path: path.to_path_buf(),
             });
        }

        let frontmatter_str = parts[1];
        let content = parts[2].trim().to_string();

        let metadata: FileSkillMetadata = serde_yaml::from_str(frontmatter_str)
            .with_context(|| format!("Failed to parse frontmatter in {:?}", path))?;

        Ok(FileSkill {
            metadata,
            content,
            path: path.to_path_buf(),
        })
    }

    pub fn to_prompt_section(&self) -> String {
        format!(
            "### Skill: {}\n\n{}\n\n{}",
            self.metadata.name,
            self.metadata.description,
            self.content
        )
    }
}

/// Load file-based skills from a directory
pub fn load_skills_from_dir(dir: &Path) -> Result<Vec<FileSkill>> {
    let mut skills = Vec::new();
    if !dir.exists() {
        return Ok(skills);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.exists() {
                match FileSkill::load_from_file(&skill_file) {
                    Ok(skill) => skills.push(skill),
                    Err(e) => warn!("Failed to load skill from {:?}: {}", skill_file, e),
                }
            }
        }
    }

    skills.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
    Ok(skills)
}

/// Format file-based skills as a prompt section
pub fn format_skills_prompt(skills: &[FileSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut prompt = String::from("\n\n## Available Skills\n\nThe following skills are available to you. Each skill provides specific commands and usage instructions.\n\n");

    for skill in skills {
        prompt.push_str(&skill.to_prompt_section());
        prompt.push_str("\n\n---\n\n");
    }

    prompt
}

/// Import file-based skills from one directory to another
pub fn import_skills_from_dir(source_dir: &Path, target_dir: &Path, overwrite: bool) -> Result<usize> {
    if !source_dir.exists() {
        anyhow::bail!("Source directory does not exist: {:?}", source_dir);
    }
    if !target_dir.exists() {
        fs::create_dir_all(target_dir)?;
    }

    let mut count = 0;
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.exists() {
                let skill_name = path.file_name().unwrap();
                let target_skill_dir = target_dir.join(skill_name);

                if target_skill_dir.exists() {
                    if !overwrite {
                        continue;
                    }
                } else {
                    fs::create_dir_all(&target_skill_dir)?;
                }

                for sub_entry in fs::read_dir(&path)? {
                    let sub_entry = sub_entry?;
                    let sub_path = sub_entry.path();
                    if sub_path.is_file() {
                        fs::copy(&sub_path, target_skill_dir.join(sub_entry.file_name()))?;
                    } else if sub_path.is_dir() {
                        let sub_dir_name = sub_entry.file_name();
                        let target_sub_dir = target_skill_dir.join(sub_dir_name);
                        fs::create_dir_all(&target_sub_dir)?;
                        for deep_entry in fs::read_dir(&sub_path)? {
                            let deep_entry = deep_entry?;
                            if deep_entry.path().is_file() {
                                fs::copy(deep_entry.path(), target_sub_dir.join(deep_entry.file_name()))?;
                            }
                        }
                    }
                }
                count += 1;
            }
        }
    }
    Ok(count)
}
