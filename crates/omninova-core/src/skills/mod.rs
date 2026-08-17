use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
    /// Marketplace version/tag when the API exposes one.
    pub version: Option<String>,
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
        .timeout(Duration::from_secs(30))
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
        .get("description_zh")
        .and_then(|s| s.as_str())
        .filter(|s| !s.trim().is_empty())
        .or_else(|| v.get("summary").and_then(|s| s.as_str()))
        .or_else(|| v.get("description").and_then(|s| s.as_str()))
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
    let version = v
        .get("version")
        .or_else(|| v.get("latestVersion"))
        .or_else(|| v.get("tag"))
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
        version,
    })
}

fn parse_skillhub_items(body: &serde_json::Value) -> Vec<SkillHubItem> {
    let data = body.get("data").unwrap_or(body);
    data.get("skills")
        .or_else(|| data.get("top10"))
        .or_else(|| data.get("items"))
        .and_then(|items| items.as_array())
        .map(|items| items.iter().filter_map(parse_skillhub_item).collect())
        .unwrap_or_default()
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
/// `source == "featured"` requests the highest-scoring entries from the
/// regular marketplace. Contest endpoints legitimately return an empty list
/// between events and therefore cannot serve as the product catalog.
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
    let mut query = vec![format!("page={page}"), format!("pageSize={page_size}")];
    if source == "featured" {
        query.push("sortBy=score".to_string());
    }
    if let Some(category) = category.map(str::trim).filter(|value| !value.is_empty()) {
        query.push(format!("category={}", urlencoding::encode(category)));
    }
    if let Some(keyword) = keyword.map(str::trim).filter(|value| !value.is_empty()) {
        query.push(format!("keyword={}", urlencoding::encode(keyword)));
    }
    let url = format!("{SKILLHUB_API_BASE}/api/skills?{}", query.join("&"));

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

    Ok(parse_skillhub_items(&body))
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
    validate_skillhub_slug(slug)?;
    let client = skillhub_client()?;
    let bytes = download_skillhub_package(&client, slug, namespace, version).await?;

    fs::create_dir_all(target_dir)?;
    let dest_root = target_dir.join(slug);
    let backup_root = target_dir.join(format!("{slug}.omninova-backup"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let stage_root = target_dir.join(format!(".omninova-install-{slug}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&stage_root)?;

    let extraction = (|| -> Result<usize> {
        let reader = std::io::Cursor::new(bytes);
        let mut archive =
            zip::ZipArchive::new(reader).context("Downloaded package is not a valid zip archive")?;
        if archive.len() > 2048 {
            anyhow::bail!("SkillHub package contains too many files");
        }
        const MAX_EXTRACTED_BYTES: u64 = 64 * 1024 * 1024;
        let mut extracted_bytes = 0_u64;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Some(rel) = entry.enclosed_name() else {
                anyhow::bail!("SkillHub package contains an unsafe path");
            };
            extracted_bytes = extracted_bytes.saturating_add(entry.size());
            if extracted_bytes > MAX_EXTRACTED_BYTES {
                anyhow::bail!("SkillHub package exceeds the 64 MiB extraction limit");
            }
            let outpath = stage_root.join(rel);
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

        let count = discover_skill_files(&stage_root)?.len();
        if count == 0 {
            anyhow::bail!("Package contains no SKILL.md; existing version was kept");
        }
        Ok(count)
    })();

    let count = match extraction {
        Ok(count) => count,
        Err(error) => {
            let _ = fs::remove_dir_all(&stage_root);
            return Err(error);
        }
    };

    if backup_root.exists() {
        if let Err(error) = fs::remove_dir_all(&backup_root) {
            let _ = fs::remove_dir_all(&stage_root);
            return Err(error).with_context(|| {
                format!("Failed to clear previous skill backup: {:?}", backup_root)
            });
        }
    }
    let had_previous = dest_root.exists();
    if had_previous {
        if let Err(error) = fs::rename(&dest_root, &backup_root) {
            let _ = fs::remove_dir_all(&stage_root);
            return Err(error).with_context(|| {
                format!("Failed to preserve existing skill version: {:?}", dest_root)
            });
        }
    }
    if let Err(error) = fs::rename(&stage_root, &dest_root) {
        if had_previous && backup_root.exists() {
            let _ = fs::rename(&backup_root, &dest_root);
        }
        let _ = fs::remove_dir_all(&stage_root);
        return Err(error).context("Failed to activate the validated SkillHub package");
    }
    Ok((slug.to_string(), count))
}

fn skillhub_download_url(slug: &str, namespace: Option<&str>, version: Option<&str>) -> String {
    let mut url = format!(
        "{SKILLHUB_API_BASE}/api/v1/download?slug={}",
        urlencoding::encode(slug)
    );
    if let Some(ns) = namespace.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&namespace={}", urlencoding::encode(ns)));
    }
    // SkillHub treats `tag` as a named tag. Marketplace `version` is semver
    // (e.g. 1.0.2) and must be sent as `version`, otherwise the API returns 404.
    if let Some(v) = version.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&version={}", urlencoding::encode(v)));
    }
    url
}

async fn download_skillhub_package(
    client: &reqwest::Client,
    slug: &str,
    namespace: Option<&str>,
    version: Option<&str>,
) -> Result<Vec<u8>> {
    let version = version.filter(|s| !s.is_empty());
    let mut urls = vec![skillhub_download_url(slug, namespace, version)];
    if version.is_some() {
        urls.push(skillhub_download_url(slug, namespace, None));
    }

    let mut last_error = None;
    for url in urls {
        match fetch_skillhub_package(client, &url).await {
            Ok(bytes) => return Ok(bytes),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("SkillHub download failed")))
}

async fn fetch_skillhub_package(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    const MAX_PACKAGE_BYTES: u64 = 25 * 1024 * 1024;
    let resp = client
        .get(url)
        .send()
        .await
        .context("SkillHub download request failed")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let detail = body.trim();
        if detail.is_empty() {
            anyhow::bail!("SkillHub download returned HTTP {status}");
        }
        anyhow::bail!(
            "SkillHub download returned HTTP {status}: {}",
            detail.chars().take(180).collect::<String>()
        );
    }
    if resp.content_length().is_some_and(|size| size > MAX_PACKAGE_BYTES) {
        anyhow::bail!("SkillHub package exceeds the 25 MiB download limit");
    }
    let bytes = resp
        .bytes()
        .await
        .context("Failed to read SkillHub package bytes")?;
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        anyhow::bail!("SkillHub package exceeds the 25 MiB download limit");
    }
    Ok(bytes.to_vec())
}

fn validate_skillhub_slug(slug: &str) -> Result<()> {
    let slug = slug.trim();
    if slug.is_empty() {
        anyhow::bail!("skill slug must not be empty");
    }
    if slug.len() > 128
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        anyhow::bail!("skill slug contains unsupported path characters");
    }
    Ok(())
}

/// Swap the active skill directory with the one-version backup kept by install.
/// Calling rollback again toggles back to the previously active version.
pub fn skillhub_rollback(target_dir: &Path, slug: &str) -> Result<(String, usize)> {
    validate_skillhub_slug(slug)?;
    let dest_root = target_dir.join(slug);
    let backup_root = target_dir.join(format!("{slug}.omninova-backup"));
    if !dest_root.exists() {
        anyhow::bail!("Cannot roll back a skill that is not installed");
    }
    if !backup_root.exists() {
        anyhow::bail!("No previous version is available for rollback");
    }
    let swap_root = target_dir.join(format!(".omninova-rollback-{slug}-{}", std::process::id()));
    if swap_root.exists() {
        fs::remove_dir_all(&swap_root)?;
    }
    fs::rename(&dest_root, &swap_root).context("Failed to prepare the current skill for rollback")?;
    if let Err(error) = fs::rename(&backup_root, &dest_root) {
        let _ = fs::rename(&swap_root, &dest_root);
        return Err(error).context("Failed to restore the previous skill version");
    }
    if let Err(error) = fs::rename(&swap_root, &backup_root) {
        // Restore the pre-rollback layout so an error never leaves the UI and
        // filesystem disagreeing about which version is active.
        let _ = fs::rename(&dest_root, &backup_root);
        let _ = fs::rename(&swap_root, &dest_root);
        return Err(error).context("Rollback was reverted because the replaced version could not be preserved");
    }
    let count = discover_skill_files(&dest_root)?.len();
    Ok((slug.to_string(), count))
}

/// Remove an installed SkillHub package and the one-version rollback backup.
/// The validated slug keeps deletion strictly inside the configured skills root.
pub fn skillhub_remove(target_dir: &Path, slug: &str) -> Result<String> {
    validate_skillhub_slug(slug)?;
    let dest_root = target_dir.join(slug);
    let backup_root = target_dir.join(format!("{slug}.omninova-backup"));
    if !dest_root.exists() && !backup_root.exists() {
        anyhow::bail!("Skill is not installed");
    }
    if dest_root.exists() {
        fs::remove_dir_all(&dest_root)
            .with_context(|| format!("Failed to remove installed skill: {:?}", dest_root))?;
    }
    if backup_root.exists() {
        fs::remove_dir_all(&backup_root)
            .with_context(|| format!("Failed to remove skill rollback backup: {:?}", backup_root))?;
    }
    Ok(slug.to_string())
}

/// Return the set of top-level directory names under `dir` that contain a skill.
pub fn installed_skill_slugs(dir: &Path) -> Result<Vec<String>> {
    let mut slugs = std::collections::BTreeSet::new();
    for skill_file in discover_skill_files(dir)? {
        if let Ok(rel) = skill_file.strip_prefix(dir) {
            if let Some(first) = rel.components().next() {
                let slug = first.as_os_str().to_string_lossy().to_string();
                if !slug.ends_with(".omninova-backup") && !slug.starts_with(".omninova-") {
                    slugs.insert(slug);
                }
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

#[cfg(test)]
mod skillhub_tests {
    use super::{parse_skillhub_items, skillhub_download_url, skillhub_remove};
    use serde_json::json;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_regular_marketplace_envelope() {
        let body = json!({
            "code": 0,
            "data": {
                "skills": [{
                    "name": "会议助手",
                    "slug": "meeting-assistant",
                    "description": "English fallback",
                    "description_zh": "中文描述",
                    "downloads": 42,
                    "category": "office-efficiency",
                    "version": "1.2.3",
                    "namespace": {
                        "handle": "nova-lab",
                        "publicSlug": "meeting-assistant"
                    }
                }],
                "total": 1
            }
        });

        let items = parse_skillhub_items(&body);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].slug, "meeting-assistant");
        assert_eq!(items[0].namespace.as_deref(), Some("nova-lab"));
        assert_eq!(items[0].description, "中文描述");
    }

    #[test]
    fn falls_back_when_localized_description_is_empty() {
        let body = json!({
            "data": {
                "skills": [{
                    "name": "Fallback Skill",
                    "slug": "fallback-skill",
                    "description_zh": "   ",
                    "description": "Readable fallback"
                }]
            }
        });

        let items = parse_skillhub_items(&body);
        assert_eq!(items[0].description, "Readable fallback");
    }

    #[test]
    fn download_url_sends_marketplace_version_as_version_not_tag() {
        let url = skillhub_download_url(
            "web-tools-guide",
            Some("user_ec205dbb"),
            Some("1.0.2"),
        );
        assert!(url.contains("slug=web-tools-guide"));
        assert!(url.contains("namespace=user_ec205dbb"));
        assert!(url.contains("version=1.0.2"));
        assert!(!url.contains("tag="));
    }

    #[test]
    fn remove_deletes_active_skill_and_rollback_backup() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("omninova-skill-remove-{nonce}"));
        let active = root.join("demo-skill");
        let backup = root.join("demo-skill.omninova-backup");
        fs::create_dir_all(&active).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(active.join("SKILL.md"), "---\nname: demo\ndescription: demo\n---\nbody").unwrap();
        fs::write(backup.join("SKILL.md"), "---\nname: demo\ndescription: old\n---\nbody").unwrap();

        let removed = skillhub_remove(&root, "demo-skill").unwrap();

        assert_eq!(removed, "demo-skill");
        assert!(!active.exists());
        assert!(!backup.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remove_rejects_unsafe_slug() {
        let error = skillhub_remove(&std::env::temp_dir(), "../outside").unwrap_err();
        assert!(error.to_string().contains("unsupported path characters"));
    }
}
