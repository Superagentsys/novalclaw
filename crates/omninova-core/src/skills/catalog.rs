use super::{discover_skill_files, is_runtime_skill_dir_name, skills_generation, Skill};
use crate::config::{resolve_configured_skills_dir, Config};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

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

/// Upper bound on how long a cached catalog may serve edits made outside the
/// app. In-app installs, removals, rollbacks, imports and skills-config saves
/// all bump [`skills_generation`], which is part of the cache key, so those are
/// reflected immediately and never wait for this.
const CATALOG_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(PartialEq, Eq)]
struct CatalogCacheKey {
    skills_dir: PathBuf,
    enabled: bool,
    generation: u64,
}

struct CachedCatalog {
    key: CatalogCacheKey,
    loaded_at: Instant,
    catalog: Arc<SkillCatalog>,
}

impl CachedCatalog {
    /// Whether this entry may still answer for `key`. `now` is a parameter so
    /// the TTL edge is testable without sleeping.
    fn serves(&self, key: &CatalogCacheKey, now: Instant) -> bool {
        self.key == *key && now.duration_since(self.loaded_at) < CATALOG_CACHE_TTL
    }
}

/// Single slot: production has one skills store, and a mirrored marketplace
/// catalog is tens of megabytes, so keeping a map keyed by directory would
/// trade a rare hit for unbounded memory.
fn catalog_cache() -> &'static Mutex<Option<CachedCatalog>> {
    static CACHE: OnceLock<Mutex<Option<CachedCatalog>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Drops the cached catalog so the next read walks the store again. For paths
/// that change skills without going through a generation bump, such as a
/// user-triggered refresh after editing the directory by hand.
pub fn invalidate_skill_catalog_cache() {
    if let Ok(mut slot) = catalog_cache().lock() {
        *slot = None;
    }
}

/// Catalog for the configured store, reused across calls.
///
/// Walking a mirrored marketplace is expensive and unavoidable: 11k skills
/// means ~89k directory entries and ~7s on APFS, most of it in the walk rather
/// than the frontmatter parse. The catalog is rebuilt several times per agent
/// turn (prompt assembly, `use_skill`, command palette), so it is cached whole.
pub fn cached_skill_catalog(config: &Config) -> Arc<SkillCatalog> {
    let key = CatalogCacheKey {
        skills_dir: resolve_configured_skills_dir(config),
        enabled: config.skills.open_skills_enabled,
        generation: skills_generation(),
    };
    if let Ok(slot) = catalog_cache().lock() {
        if let Some(cached) = slot.as_ref() {
            if cached.serves(&key, Instant::now()) {
                return Arc::clone(&cached.catalog);
            }
        }
    }
    // Built without the lock held: a 7s rebuild must not block unrelated
    // readers, and a duplicated concurrent build is harmless.
    let catalog = Arc::new(build_skill_catalog(config));
    if let Ok(mut slot) = catalog_cache().lock() {
        *slot = Some(CachedCatalog {
            key,
            loaded_at: Instant::now(),
            catalog: Arc::clone(&catalog),
        });
    }
    catalog
}

/// Owned catalog for callers that need to mutate or serialize it. Prefer
/// [`cached_skill_catalog`] on read-only paths: cloning 11k entries costs
/// ~30ms, which is cheap next to a rebuild but not free.
pub fn list_skill_catalog(config: &Config) -> SkillCatalog {
    (*cached_skill_catalog(config)).clone()
}

/// Rebuild the catalog from the canonical Skill Store. No session state.
fn build_skill_catalog(config: &Config) -> SkillCatalog {
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

/// Ceiling on catalog entries written into the system prompt. A mirrored skill
/// marketplace can hold tens of thousands of entries; listing every one grew
/// the system prompt to 3.2 MB (~4.1M estimated tokens), which on its own
/// exceeded every provider's input budget and made the session unusable.
/// Entries past the cap stay reachable through `use_skill`'s `query` search.
pub const DEFAULT_CATALOG_PROMPT_LIMIT: usize = 150;

/// Ceiling on each entry's description in the prompt. Skill descriptions are
/// authored for humans and run past 2000 characters in practice.
pub const DEFAULT_CATALOG_DESCRIPTION_LIMIT: usize = 160;

/// Cap on matches returned by a single `use_skill` catalog search.
pub const CATALOG_SEARCH_LIMIT: usize = 25;

pub fn catalog_prompt_section(catalog: &SkillCatalog) -> String {
    catalog_prompt_section_with_limits(
        catalog,
        DEFAULT_CATALOG_PROMPT_LIMIT,
        DEFAULT_CATALOG_DESCRIPTION_LIMIT,
    )
}

/// `entry_limit` / `description_limit` of 0 mean unlimited, matching how the
/// rest of the config treats budget ceilings.
pub fn catalog_prompt_section_with_limits(
    catalog: &SkillCatalog,
    entry_limit: usize,
    description_limit: usize,
) -> String {
    if !catalog.open_skills_enabled {
        return String::new();
    }
    let mut prompt = String::from(
        "\n\n## Skill Catalog\n\n\
You have a catalog of installed skills. Do not assume their full instructions yet.\n\
To load one skill's instructions, call the `use_skill` tool with its `skill_id`.\n\
At most one skill may be active at a time. Prefer matching the user task; if none match, continue without a skill.\n\n",
    );
    let visible = prompt_ordered_entries(catalog);
    if visible.is_empty() {
        prompt.push_str("No runtime-visible skills are currently installed.\n");
        return prompt;
    }
    let listed = if entry_limit == 0 {
        visible.len()
    } else {
        entry_limit.min(visible.len())
    };
    for entry in &visible[..listed] {
        prompt.push_str(&format!(
            "- `{id}` / `{alias}` — {name}: {description}\n",
            id = entry.id,
            alias = entry.command_alias,
            name = entry.display_name,
            description = truncate_description(&one_line(&entry.description), description_limit),
        ));
    }
    let hidden = visible.len() - listed;
    if hidden > 0 {
        prompt.push_str(&format!(
            "\n{hidden} more installed skills are not listed above. \
The catalog is too large to inline, so search it instead of guessing an id: \
call `use_skill` with a `query` such as {{\"query\": \"pdf table extraction\"}}, \
then call `use_skill` again with a `skill_id` from the results.\n"
        ));
    }
    prompt
}

/// Personal and system skills come before mirrored marketplace entries so a
/// truncated listing keeps the skills a user actually authored.
fn prompt_ordered_entries(catalog: &SkillCatalog) -> Vec<&SkillCatalogEntry> {
    let mut visible: Vec<&SkillCatalogEntry> = catalog
        .entries
        .iter()
        .filter(|entry| entry.runtime_visible)
        .collect();
    visible.sort_by_key(|entry| source_prompt_rank(&entry.source));
    visible
}

fn source_prompt_rank(source: &str) -> u8 {
    match source {
        "system" => 0,
        "personal" => 1,
        _ => 2,
    }
}

/// Search the whole catalog, including entries the prompt had to omit.
/// Ranks exact id/slug hits first, then name matches, then description matches.
pub fn search_skill_catalog<'a>(
    catalog: &'a SkillCatalog,
    query: &str,
    limit: usize,
) -> Vec<&'a SkillCatalogEntry> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let terms: Vec<&str> = needle.split_whitespace().collect();
    let mut scored: Vec<(u32, &'a SkillCatalogEntry)> = catalog
        .entries
        .iter()
        .filter(|entry| entry.runtime_visible)
        .filter_map(|entry| {
            let score = score_entry(entry, &needle, &terms);
            (score > 0).then_some((score, entry))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.display_name.cmp(&b.1.display_name))
            .then(a.1.id.cmp(&b.1.id))
    });
    scored
        .into_iter()
        .take(if limit == 0 { CATALOG_SEARCH_LIMIT } else { limit })
        .map(|(_, entry)| entry)
        .collect()
}

fn score_entry(entry: &SkillCatalogEntry, needle: &str, terms: &[&str]) -> u32 {
    let slug = entry.slug.to_lowercase();
    let name = entry.display_name.to_lowercase();
    let description = entry.description.to_lowercase();
    if slug == needle || entry.id.to_lowercase() == needle {
        return u32::MAX;
    }
    let mut score = 0u32;
    if slug.contains(needle) {
        score += 200;
    }
    if name.contains(needle) {
        score += 150;
    }
    for term in terms {
        if slug.contains(term) {
            score += 30;
        }
        if name.contains(term) {
            score += 20;
        }
        if description.contains(term) {
            score += 5;
        }
    }
    score
}

/// Truncates on a character boundary so multi-byte descriptions stay valid.
fn truncate_description(text: &str, limit: usize) -> String {
    if limit == 0 || text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit).collect();
    format!("{}…", kept.trim_end())
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

#[cfg(test)]
mod cache_tests {
    use super::*;

    fn key(dir: &str, enabled: bool, generation: u64) -> CatalogCacheKey {
        CatalogCacheKey {
            skills_dir: PathBuf::from(dir),
            enabled,
            generation,
        }
    }

    fn cached(key: CatalogCacheKey, loaded_at: Instant) -> CachedCatalog {
        CachedCatalog {
            key,
            loaded_at,
            catalog: Arc::new(SkillCatalog {
                generation: 1,
                open_skills_enabled: true,
                skills_dir: PathBuf::from("/store"),
                entries: Vec::new(),
            }),
        }
    }

    #[test]
    fn an_unchanged_key_is_served_from_cache() {
        let now = Instant::now();
        let entry = cached(key("/store", true, 7), now);
        assert!(entry.serves(&key("/store", true, 7), now));
    }

    #[test]
    fn a_skill_mutation_is_never_served_from_cache() {
        let now = Instant::now();
        let entry = cached(key("/store", true, 7), now);
        // bump_skills_generation() moves this, so installs and removals are
        // visible on the next read rather than after the TTL.
        assert!(!entry.serves(&key("/store", true, 8), now));
    }

    #[test]
    fn another_store_or_toggle_is_never_served_from_cache() {
        let now = Instant::now();
        let entry = cached(key("/store", true, 7), now);
        assert!(!entry.serves(&key("/other-store", true, 7), now));
        assert!(!entry.serves(&key("/store", false, 7), now));
    }

    #[test]
    fn an_expired_entry_is_rebuilt() {
        let loaded_at = Instant::now();
        let entry = cached(key("/store", true, 7), loaded_at);
        let wanted = key("/store", true, 7);
        assert!(entry.serves(&wanted, loaded_at + CATALOG_CACHE_TTL - Duration::from_millis(1)));
        assert!(!entry.serves(&wanted, loaded_at + CATALOG_CACHE_TTL));
    }
}
