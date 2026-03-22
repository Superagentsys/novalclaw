use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Context, Result};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub homepage: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub metadata: SkillMetadata,
    pub content: String,
    pub path: PathBuf,
}

impl Skill {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("Failed to read skill file: {:?}", path))?;

        let parts: Vec<&str> = raw.splitn(3, "---").collect();
        if parts.len() < 3 {
             let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
             return Ok(Skill {
                 metadata: SkillMetadata {
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

        let metadata: SkillMetadata = serde_yaml::from_str(frontmatter_str)
            .with_context(|| format!("Failed to parse frontmatter in {:?}", path))?;

        Ok(Skill {
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

pub fn load_skills_from_dir(dir: &Path) -> Result<Vec<Skill>> {
    let mut skills = Vec::new();
    if !dir.exists() {
        return Ok(skills);
    }

    for skill_file in discover_skill_files(dir)? {
        match Skill::load_from_file(&skill_file) {
            Ok(skill) => skills.push(skill),
            Err(e) => warn!("Failed to load skill from {:?}: {}", skill_file, e),
        }
    }
    
    skills.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
    Ok(skills)
}

pub fn format_skills_prompt(skills: &[Skill]) -> String {
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

pub fn import_skills_from_dir(source_dir: &Path, target_dir: &Path, overwrite: bool) -> Result<usize> {
    if !source_dir.exists() {
        anyhow::bail!("Source directory does not exist: {:?}", source_dir);
    }
    if !target_dir.exists() {
        fs::create_dir_all(target_dir)?;
    }

    let skill_files = discover_skill_files(source_dir)?;
    let mut count = 0;
    for skill_file in skill_files {
        let Some(skill_root) = skill_file.parent() else {
            continue;
        };
        let relative_skill_root = skill_root
            .strip_prefix(source_dir)
            .unwrap_or(skill_root);
        let target_skill_dir = target_dir.join(relative_skill_root);

        if target_skill_dir.exists() && !overwrite {
            continue;
        }
        if target_skill_dir.exists() {
            fs::remove_dir_all(&target_skill_dir)?;
        }
        copy_dir_recursive(skill_root, &target_skill_dir)?;
        count += 1;
    }
    Ok(count)
}

fn discover_skill_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    discover_skill_files_inner(root, &mut files)?;
    files.sort();
    files.dedup();
    Ok(files)
}

fn discover_skill_files_inner(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            discover_skill_files_inner(&path, out)?;
            continue;
        }
        if path.is_file() && is_skill_file_path(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_skill_file_path(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().eq_ignore_ascii_case("SKILL.md"))
        .unwrap_or(false)
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if source_path.is_file() {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}
