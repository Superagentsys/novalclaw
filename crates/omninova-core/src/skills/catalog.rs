use super::{discover_skill_files, is_runtime_skill_dir_name, skills_generation, Skill};
use crate::config::{resolve_configured_skills_dir, Config};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_ACTIVE_SKILLS: usize = 1;
pub const SKILL_ID_PREFIX: &str = "skill:";
pub const SYSTEM_ID_PREFIX: &str = "system:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillResource {
    pub name: String,
    pub kind: String,
    pub locator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalogEntry {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub source: String,
    pub command_alias: String,
    pub installed: bool,
    pub enabled: bool,
    pub runtime_visible: bool,
    pub skill_path: PathBuf,
    pub has_scripts: bool,
    pub resources: Vec<SkillResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalog {
    pub generation: u64,
    pub open_skills_enabled: bool,
    pub skills_dir: PathBuf,
    pub entries: Vec<SkillCatalogEntry>,
}

impl SkillCatalog {
    pub fn get(&self, skill_id: &str) -> Option<&SkillCatalogEntry> {
        let wanted = normalize_skill_id(skill_id);
        self.entries.iter().find(|entry| {
            entry.id == wanted
                || entry.slug.eq_ignore_ascii_case(wanted.trim_start_matches(SKILL_ID_PREFIX))
                || entry.command_alias.trim_start_matches('/') == wanted.trim_start_matches('/')
        })
    }
}

/// Rebuild the catalog from the canonical Skill Store. No session state.
pub fn list_skill_catalog(config: &Config) -> SkillCatalog {
    let skills_dir = resolve_configured_skills_dir(config);
    let enabled = config.skills.open_skills_enabled;
    let mut entries = Vec::new();
    if let Ok(files) = discover_skill_files(&skills_dir) {
        let mut used_ids = std::collections::BTreeSet::new();
        for path in files {
            if let Some(entry) = catalog_entry_from_skill_file(&skills_dir, &path, enabled) {
                let mut entry = entry;
                if !used_ids.insert(entry.id.clone()) {
                    let mut suffix = 2u32;
                    let base = entry.id.clone();
                    loop {
                        let candidate = format!("{base}-{suffix}");
                        if used_ids.insert(candidate.clone()) {
                            entry.id = candidate;
                            break;
                        }
                        suffix += 1;
                    }
                }
                entries.push(entry);
            }
        }
    }
    entries.sort_by(|a, b| a.display_name.cmp(&b.display_name).then(a.id.cmp(&b.id)));
    SkillCatalog {
        generation: skills_generation(),
        open_skills_enabled: enabled,
        skills_dir,
        entries,
    }
}

pub fn catalog_prompt_section(catalog: &SkillCatalog) -> String {
    if !catalog.open_skills_enabled {
        return String::new();
    }
    let mut prompt = String::from(
        "\n\n## Skill Catalog\n\n\
You have a catalog of installed skills. Do not assume their full instructions yet.\n\
To load one skill's instructions, call the `use_skill` tool with its `skill_id`.\n\
At most one skill may be active at a time. Prefer matching the user task; if none match, continue without a skill.\n\n",
    );
    if catalog.entries.is_empty() {
        prompt.push_str("No runtime-visible skills are currently installed.\n");
        return prompt;
    }
    for entry in &catalog.entries {
        if !entry.runtime_visible {
            continue;
        }
        prompt.push_str(&format!(
            "- `{id}` / `{alias}` — {name}: {description}\n",
            id = entry.id,
            alias = entry.command_alias,
            name = entry.display_name,
            description = one_line(&entry.description),
        ));
    }
    prompt
}

pub fn normalize_skill_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with(SKILL_ID_PREFIX) || trimmed.starts_with(SYSTEM_ID_PREFIX) {
        return trimmed.to_string();
    }
    let slug = trimmed.trim_start_matches('/');
    format!("{SKILL_ID_PREFIX}{slug}")
}

pub fn is_safe_skill_locator(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 180 {
        return false;
    }
    let slug = trimmed
        .trim_start_matches(SKILL_ID_PREFIX)
        .trim_start_matches('/');
    if slug.is_empty() || slug == "." || slug == ".." {
        return false;
    }
    if Path::new(slug).is_absolute() {
        return false;
    }
    !slug.split(['/', '\\']).any(|part| {
        part.is_empty()
            || part == "."
            || part == ".."
            || part.contains(':')
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    })
}

fn catalog_entry_from_skill_file(
    skills_dir: &Path,
    skill_path: &Path,
    open_skills_enabled: bool,
) -> Option<SkillCatalogEntry> {
    let rel = skill_path.strip_prefix(skills_dir).ok()?;
    let mut parts = Vec::new();
    for component in rel.components() {
        let name = component.as_os_str().to_string_lossy();
        if name.eq_ignore_ascii_case("SKILL.md") {
            continue;
        }
        if !is_runtime_skill_dir_name(&name) {
            return None;
        }
        parts.push(name.into_owned());
    }
    if parts.is_empty() {
        let stem = skill_path.file_stem()?.to_string_lossy().into_owned();
        parts.push(stem);
    }
    let slug = parts.join("/");
    if !is_safe_skill_locator(&slug) {
        return None;
    }
    let meta = read_skill_frontmatter(skill_path);
    let skill_root = skill_path.parent().unwrap_or(skill_path);
    let resources = index_skill_resources(skill_root);
    let has_scripts = resources.iter().any(|item| item.kind == "script");
    let source = classify_skill_source(skill_root);
    Some(SkillCatalogEntry {
        id: format!("{SKILL_ID_PREFIX}{slug}"),
        display_name: meta
            .name
            .unwrap_or_else(|| parts.last().cloned().unwrap_or_else(|| slug.clone())),
        description: meta.description.unwrap_or_default(),
        version: meta.version.unwrap_or_default(),
        source,
        command_alias: format!("/{slug}"),
        installed: true,
        enabled: open_skills_enabled,
        runtime_visible: open_skills_enabled,
        skill_path: skill_path.to_path_buf(),
        slug,
        has_scripts,
        resources,
    })
}

struct FrontmatterBits {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
}

fn read_skill_frontmatter(path: &Path) -> FrontmatterBits {
    let Ok(raw) = fs::read_to_string(path) else {
        return FrontmatterBits {
            name: None,
            description: None,
            version: None,
        };
    };
    let parts: Vec<&str> = raw.splitn(3, "---").collect();
    if parts.len() < 3 {
        return FrontmatterBits {
            name: None,
            description: None,
            version: None,
        };
    }
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(parts[1]) else {
        return FrontmatterBits {
            name: None,
            description: None,
            version: None,
        };
    };
    let Some(map) = value.as_mapping() else {
        return FrontmatterBits {
            name: None,
            description: None,
            version: None,
        };
    };
    FrontmatterBits {
        name: yaml_string(map.get(&serde_yaml::Value::String("name".into()))),
        description: yaml_string(map.get(&serde_yaml::Value::String("description".into()))),
        version: yaml_string(map.get(&serde_yaml::Value::String("version".into()))),
    }
}

fn yaml_string(value: Option<&serde_yaml::Value>) -> Option<String> {
    let value = value?;
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    if let Some(number) = value.as_i64() {
        return Some(number.to_string());
    }
    if let Some(number) = value.as_f64() {
        return Some(number.to_string());
    }
    None
}

fn classify_skill_source(skill_root: &Path) -> String {
    let marker_names = [
        "skillhub.json",
        ".skillhub",
        "SKILLHUB.md",
        "marketplace.json",
    ];
    if marker_names
        .iter()
        .any(|name| skill_root.join(name).is_file())
    {
        return "installed".to_string();
    }
    if skill_root.join("scripts").is_dir() || skill_root.join("references").is_dir() {
        return "installed".to_string();
    }
    "personal".to_string()
}

fn index_skill_resources(skill_root: &Path) -> Vec<SkillResource> {
    let mut resources = Vec::new();
    let kinds = [
        ("references", "reference"),
        ("templates", "template"),
        ("assets", "asset"),
        ("scripts", "script"),
    ];
    for (dir_name, kind) in kinds {
        let dir = skill_root.join(dir_name);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            resources.push(SkillResource {
                name: name.clone(),
                kind: kind.to_string(),
                locator: format!("{dir_name}/{name}"),
            });
        }
    }
    resources.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    resources
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Approximate prompt size for inflation regressions (not a billed tokenizer).
pub fn estimate_prompt_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

pub fn load_skill_instructions(entry: &SkillCatalogEntry) -> anyhow::Result<Skill> {
    Skill::load_from_file(&entry.skill_path)
}

pub fn resource_index_prompt(resources: &[SkillResource], has_scripts: bool) -> String {
    let mut out = String::from("\n### Skill resources (index only; do not assume file contents)\n");
    if resources.is_empty() {
        out.push_str("- none\n");
    } else {
        for resource in resources {
            out.push_str(&format!(
                "- {} ({}) locator={}\n",
                resource.name, resource.kind, resource.locator
            ));
        }
    }
    if has_scripts {
        out.push_str(
            "This skill lists scripts, but they must not be executed automatically in this runtime.\n",
        );
    }
    out
}

pub fn source_badge_label(source: &str) -> &'static str {
    match source {
        "system" => "系统",
        "personal" => "个人",
        "installed" => "已安装",
        _ => "已安装",
    }
}
