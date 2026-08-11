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

// ---------------------------------------------------------------------------
// SkillHub (https://skillhub.cn) integration
// ---------------------------------------------------------------------------

/// SkillHub public API base host.
pub const SKILLHUB_API_BASE: &str = "https://api.skillhub.cn";

/// A single skill entry as surfaced by the SkillHub marketplace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillHubItem {
    pub name: String,
    /// Public slug used for download (namespace-scoped publicSlug when present).
    pub slug: String,
    /// Namespace handle, required to disambiguate identically named slugs.
    pub namespace: Option<String>,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub category: Option<String>,
}

/// A SkillHub category (level-1 taxonomy).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillHubCategory {
    pub key: String,
    pub name: String,
}

fn skillhub_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("OmniNovaClaw/1.0 (+https://skillhub.cn)")
        .build()
        .context("Failed to build SkillHub HTTP client")
}

fn parse_skillhub_item(v: &serde_json::Value) -> Option<SkillHubItem> {
    let ns = v.get("namespace");
    let public_slug = ns
        .and_then(|n| n.get("publicSlug"))
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty());
    let slug = public_slug
        .or_else(|| v.get("slug").and_then(|s| s.as_str()))
        .map(|s| s.to_string())?;
    let namespace = ns
        .and_then(|n| n.get("handle"))
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let name = v
        .get("name")
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&slug)
        .to_string();
    let description = v
        .get("summary")
        .or_else(|| v.get("description"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let icon_url = v
        .get("iconUrl")
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let downloads = v.get("downloads").and_then(|d| d.as_u64()).unwrap_or(0);
    let category = v
        .get("category")
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    Some(SkillHubItem {
        name,
        slug,
        namespace,
        description,
        icon_url,
        downloads,
        category,
    })
}

/// Fetch the level-1 category taxonomy from SkillHub.
pub async fn skillhub_categories() -> Result<Vec<SkillHubCategory>> {
    let client = skillhub_client()?;
    let url = format!("{SKILLHUB_API_BASE}/api/v1/categories");
    let body: serde_json::Value = client
        .get(&url)
        .send()
        .await
        .context("SkillHub categories request failed")?
        .error_for_status()
        .context("SkillHub categories returned an error status")?
        .json()
        .await
        .context("Failed to decode SkillHub categories")?;
    let items = body
        .get("items")
        .and_then(|i| i.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let key = c.get("key").and_then(|s| s.as_str())?.to_string();
                    let name = c
                        .get("name")
                        .and_then(|s| s.as_str())
                        .unwrap_or(&key)
                        .to_string();
                    Some(SkillHubCategory { key, name })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(items)
}

/// List skills from SkillHub.
///
/// * `source == "featured"` uses the official curated top list.
/// * otherwise the general marketplace catalog is used.
///
/// When `category` or `keyword` is supplied the results are filtered
/// client-side over a wider fetch, so pagination is best-effort.
pub async fn skillhub_list(
    source: &str,
    category: Option<&str>,
    keyword: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<Vec<SkillHubItem>> {
    let client = skillhub_client()?;
    let page = page.max(1);
    let page_size = page_size.clamp(1, 60);
    let has_filter = category.map(|c| !c.is_empty()).unwrap_or(false)
        || keyword.map(|k| !k.trim().is_empty()).unwrap_or(false);

    let url = if source == "featured" {
        format!("{SKILLHUB_API_BASE}/api/v1/contest/top")
    } else if has_filter {
        // Fetch a wide page and filter locally.
        format!("{SKILLHUB_API_BASE}/api/v1/contest/skills?page=1&pageSize=60")
    } else {
        format!(
            "{SKILLHUB_API_BASE}/api/v1/contest/skills?page={page}&pageSize={page_size}"
        )
    };

    let body: serde_json::Value = client
        .get(&url)
        .send()
        .await
        .context("SkillHub list request failed")?
        .error_for_status()
        .context("SkillHub list returned an error status")?
        .json()
        .await
        .context("Failed to decode SkillHub list")?;

    // Locate the array regardless of the specific envelope shape.
    let data = body.get("data").unwrap_or(&body);
    let arr = data
        .get("top10")
        .or_else(|| data.get("items"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items: Vec<SkillHubItem> = arr.iter().filter_map(parse_skillhub_item).collect();

    if let Some(cat) = category.filter(|c| !c.is_empty()) {
        items.retain(|it| it.category.as_deref() == Some(cat));
    }
    if let Some(kw) = keyword.map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()) {
        items.retain(|it| {
            it.name.to_lowercase().contains(&kw)
                || it.slug.to_lowercase().contains(&kw)
                || it.description.to_lowercase().contains(&kw)
        });
    }

    if source == "featured" {
        return Ok(items);
    }
    if has_filter {
        // Apply local pagination over the filtered set.
        let start = ((page - 1) * page_size) as usize;
        let end = (start + page_size as usize).min(items.len());
        if start >= items.len() {
            return Ok(Vec::new());
        }
        return Ok(items[start..end].to_vec());
    }
    Ok(items)
}

/// Download a SkillHub skill package and extract it into `target_dir/<slug>`.
///
/// Returns the installed directory name and the number of `SKILL.md`
/// files discovered inside it.
pub async fn skillhub_install(
    target_dir: &Path,
    slug: &str,
    namespace: Option<&str>,
    version: Option<&str>,
) -> Result<(String, usize)> {
    if slug.trim().is_empty() {
        anyhow::bail!("skill slug must not be empty");
    }
    let mut url = format!(
        "{SKILLHUB_API_BASE}/api/v1/download?slug={}",
        urlencoding::encode(slug)
    );
    if let Some(ns) = namespace.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&namespace={}", urlencoding::encode(ns)));
    }
    if let Some(v) = version.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&tag={}", urlencoding::encode(v)));
    }

    let client = skillhub_client()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .context("SkillHub download request failed")?
        .error_for_status()
        .context("SkillHub download returned an error status")?;
    let bytes = resp
        .bytes()
        .await
        .context("Failed to read SkillHub package bytes")?;

    fs::create_dir_all(target_dir)?;
    let dest_root = target_dir.join(slug);
    if dest_root.exists() {
        fs::remove_dir_all(&dest_root)
            .with_context(|| format!("Failed to clear existing skill dir: {:?}", dest_root))?;
    }
    fs::create_dir_all(&dest_root)?;

    let reader = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).context("Downloaded package is not a valid zip archive")?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue; // skip unsafe (path traversal) entries
        };
        let outpath = dest_root.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&outpath)?;
            continue;
        }
        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&outpath)
            .with_context(|| format!("Failed to write skill file: {:?}", outpath))?;
        std::io::copy(&mut entry, &mut out)?;
    }

    let count = discover_skill_files(&dest_root)?.len();
    if count == 0 {
        anyhow::bail!("Package installed but contains no SKILL.md");
    }
    Ok((slug.to_string(), count))
}

/// Return the set of top-level directory names under `dir` that contain a skill.
pub fn installed_skill_slugs(dir: &Path) -> Result<Vec<String>> {
    let mut slugs = std::collections::BTreeSet::new();
    for skill_file in discover_skill_files(dir)? {
        if let Ok(rel) = skill_file.strip_prefix(dir) {
            if let Some(first) = rel.components().next() {
                slugs.insert(first.as_os_str().to_string_lossy().to_string());
            }
        }
    }
    Ok(slugs.into_iter().collect())
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
