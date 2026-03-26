use anyhow::{Result, Context, bail};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::OutputFormat;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub path: PathBuf,
}

/// List all installed skills
pub async fn list_skills(format: OutputFormat) -> Result<()> {
    let skills_dir = get_skills_dir()?;
    
    if !skills_dir.exists() {
        println!("{}", "No skills installed yet.".yellow());
        return Ok(());
    }

    let mut skills = Vec::new();
    
    for entry in WalkDir::new(&skills_dir).max_depth(2) {
        let entry = entry?;
        if entry.file_name() == "SKILL.md" {
            if let Ok(skill) = parse_skill_file(entry.path()) {
                skills.push(skill);
            }
        }
    }

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&skills)?);
        }
        OutputFormat::Text => {
            if skills.is_empty() {
                println!("{}", "No skills found.".yellow());
                return Ok(());
            }

            println!("{}", "Installed Skills".bold().underline());
            println!();
            
            for skill in &skills {
                println!("{} {} {}", 
                    "●".green(),
                    skill.name.bold(),
                    format!("v{}", skill.version).dimmed()
                );
                println!("  {}", skill.description);
                if !skill.tags.is_empty() {
                    println!("  {} {}", "Tags:".dimmed(), skill.tags.join(", "));
                }
                println!();
            }
            
            println!("Total: {} skills", skills.len());
        }
    }

    Ok(())
}

/// Show skill details
pub async fn show_skill(name: &str, format: OutputFormat) -> Result<()> {
    let skill = find_skill(name)?;
    
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&skill)?);
        }
        OutputFormat::Text => {
            println!("{}", "Skill Details".bold().underline());
            println!();
            println!("{} {}", "Name:".bold(), skill.name);
            println!("{} {}", "Version:".bold(), skill.version);
            println!("{} {}", "Description:".bold(), skill.description);
            if let Some(author) = skill.author {
                println!("{} {}", "Author:".bold(), author);
            }
            if !skill.tags.is_empty() {
                println!("{} {}", "Tags:".bold(), skill.tags.join(", "));
            }
            println!("{} {}", "Path:".bold(), skill.path.display());
            
            // Show SKILL.md content
            let skill_file = skill.path.join("SKILL.md");
            if skill_file.exists() {
                println!();
                println!("{}", "SKILL.md:".bold());
                let content = fs::read_to_string(&skill_file)?;
                // Skip YAML frontmatter
                if let Some(start) = content.find("---") {
                    if let Some(end) = content[start+3..].find("---") {
                        let md_content = &content[start+3+end+3..];
                        println!("{}", md_content.trim());
                    }
                }
            }
        }
    }

    Ok(())
}

/// Install skill from local directory
pub async fn install_skill(source: &str, name: Option<String>) -> Result<()> {
    let source_path = Path::new(source);
    
    if !source_path.exists() {
        bail!("Source path does not exist: {}", source);
    }

    // Validate skill
    let skill_file = source_path.join("SKILL.md");
    if !skill_file.exists() {
        bail!("Invalid skill: SKILL.md not found in {}", source);
    }

    let skill = parse_skill_file(&skill_file)?;
    let target_name = name.unwrap_or_else(|| skill.name.clone());
    
    let skills_dir = get_skills_dir()?;
    let target_path = skills_dir.join(&target_name);
    
    if target_path.exists() {
        bail!("Skill '{}' already exists. Use --force to overwrite.", target_name);
    }

    // Copy skill directory
    copy_dir_all(source_path, &target_path)?;
    
    println!("{} Skill '{}' installed successfully", "✓".green(), target_name);
    println!("  Version: {}", skill.version);
    println!("  Path: {}", target_path.display());
    
    Ok(())
}

/// Uninstall skill
pub async fn uninstall_skill(name: &str, force: bool) -> Result<()> {
    let skills_dir = get_skills_dir()?;
    let skill_path = skills_dir.join(name);
    
    if !skill_path.exists() {
        bail!("Skill '{}' not found", name);
    }

    if !force {
        print!("Are you sure you want to uninstall skill '{}'? [y/N] ", name.cyan());
        use std::io::Write;
        std::io::stdout().flush()?;
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Uninstall cancelled.");
            return Ok(());
        }
    }

    fs::remove_dir_all(&skill_path)?;
    
    println!("{} Skill '{}' uninstalled successfully", "✓".green(), name);
    
    Ok(())
}

/// Validate skill configuration
pub async fn validate_skill(path: &str) -> Result<()> {
    let skill_path = Path::new(path);
    let skill_file = skill_path.join("SKILL.md");
    
    if !skill_file.exists() {
        bail!("❌ SKILL.md not found");
    }

    print!("{} Parsing SKILL.md... ", "→".blue());
    match parse_skill_file(&skill_file) {
        Ok(skill) => {
            println!("{} OK", "✓".green());
            println!("  Name: {}", skill.name);
            println!("  Version: {}", skill.version);
            println!("  Description: {}", skill.description);
        }
        Err(e) => {
            println!("{} FAILED", "✗".red());
            bail!("Failed to parse SKILL.md: {}", e);
        }
    }

    // Check for required files
    print!("{} Checking required files... ", "→".blue());
    let mut has_errors = false;
    
    if !skill_path.join("SKILL.md").exists() {
        println!("\n  {} SKILL.md missing", "✗".red());
        has_errors = true;
    }
    
    if has_errors {
        bail!("Validation failed");
    } else {
        println!("{} OK", "✓".green());
    }

    println!("\n{} Skill is valid!", "✓".green());
    Ok(())
}

/// Package skill for distribution
pub async fn package_skill(path: &str, output: Option<String>) -> Result<()> {
    let skill_path = Path::new(path);
    
    // Validate first
    validate_skill(path).await?;
    
    let skill_file = skill_path.join("SKILL.md");
    let skill = parse_skill_file(&skill_file)?;
    
    let output_file = output.unwrap_or_else(|| {
        format!("{}-v{}.tar.gz", skill.name, skill.version)
    });
    
    // Create tar.gz
    let tar_gz = fs::File::create(&output_file)?;
    let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    
    tar.append_dir_all(&skill.name, skill_path)?;
    tar.finish()?;
    
    println!("{} Skill packaged successfully", "✓".green());
    println!("  Output: {}", output_file);
    
    Ok(())
}

// Helper functions

fn get_skills_dir() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .context("Failed to determine home directory")?;
    Ok(home.join(".omninova").join("skills"))
}

fn parse_skill_file(path: &Path) -> Result<Skill> {
    let content = fs::read_to_string(path)?;
    
    // Parse YAML frontmatter
    if let Some(start) = content.find("---") {
        if let Some(end) = content[start+3..].find("---") {
            let yaml_content = &content[start+3..start+3+end];
            let metadata: SkillMetadata = serde_yaml::from_str(yaml_content)?;
            
            return Ok(Skill {
                name: metadata.name,
                version: metadata.version,
                description: metadata.description,
                author: metadata.author,
                tags: metadata.tags.unwrap_or_default(),
                path: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            });
        }
    }
    
    bail!("Invalid SKILL.md format: missing YAML frontmatter")
}

#[derive(Debug, Deserialize)]
struct SkillMetadata {
    name: String,
    version: String,
    description: String,
    author: Option<String>,
    tags: Option<Vec<String>>,
}

fn find_skill(name: &str) -> Result<Skill> {
    let skills_dir = get_skills_dir()?;
    let skill_path = skills_dir.join(name);
    
    if !skill_path.exists() {
        bail!("Skill '{}' not found", name);
    }
    
    let skill_file = skill_path.join("SKILL.md");
    parse_skill_file(&skill_file)
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    fs::create_dir_all(dst)?;
    
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            let relative = path.strip_prefix(src)?;
            let target = dst.join(relative);
            
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            
            fs::copy(path, target)?;
        }
    }
    
    Ok(())
}

/// Execute skills commands
pub async fn execute(cmd: super::SkillsCommands, format: OutputFormat) -> Result<()> {
    match cmd {
        super::SkillsCommands::List => list_skills(format).await,
        super::SkillsCommands::Show { name } => show_skill(&name, format).await,
        super::SkillsCommands::Install { source, name } => install_skill(&source, name).await,
        super::SkillsCommands::Uninstall { name, force } => uninstall_skill(&name, force).await,
        super::SkillsCommands::Validate { path } => validate_skill(&path).await,
        super::SkillsCommands::Package { path, output } => package_skill(&path, output).await,
    }
}
