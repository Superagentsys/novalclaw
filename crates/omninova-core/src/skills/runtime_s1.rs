use super::*;
use crate::config::{
    resolve_configured_skills_dir, resolve_effective_workspace_dir,
    resolve_effective_workspace_skills_dir, Config, DEFAULT_OPEN_SKILLS_ENABLED,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("omninova-s1-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_skill(dir: &Path, slug: &str, name: &str, body: &str) {
    let root = dir.join(slug);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {name}\n---\n{body}\n"),
    )
    .unwrap();
}

fn config_with_workspace(workspace: &Path, enabled: bool) -> Config {
    let mut config = Config::default();
    config.workspace_dir = workspace.to_path_buf();
    config.skills.open_skills_enabled = enabled;
    config
}

#[test]
fn open_skills_enabled_default_matches_core_source_of_truth() {
    assert!(DEFAULT_OPEN_SKILLS_ENABLED);
    assert!(Config::default().skills.open_skills_enabled);
    let parsed: crate::config::SkillsConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(parsed.open_skills_enabled, DEFAULT_OPEN_SKILLS_ENABLED);
}

#[test]
fn normal_active_skill_is_discovered_once() {
    let root = TempDir::new("once");
    write_skill(&root.0, "skill-a", "skill-a", "use skill a");
    let loaded = load_skills_from_dir(&root.0).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].metadata.name, "skill-a");
    let slugs = installed_skill_slugs(&root.0).unwrap();
    assert_eq!(slugs, vec!["skill-a".to_string()]);
}

#[test]
fn backup_directory_is_not_discovered() {
    let root = TempDir::new("backup");
    write_skill(&root.0, "skill-a", "skill-a", "active body");
    write_skill(
        &root.0,
        "skill-a.omninova-backup",
        "skill-a",
        "old backup body",
    );
    let loaded = load_skills_from_dir(&root.0).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].content.contains("active body"));
    assert!(!loaded.iter().any(|skill| skill.content.contains("old backup body")));
    let slugs = installed_skill_slugs(&root.0).unwrap();
    assert_eq!(slugs, vec!["skill-a".to_string()]);
}

#[test]
fn rollback_staging_directory_is_not_discovered() {
    let root = TempDir::new("staging");
    write_skill(&root.0, "skill-a", "skill-a", "active");
    write_skill(&root.0, ".omninova-rollback-skill-a-1", "skill-a", "staging");
    write_skill(&root.0, ".omninova-install-skill-a-1-1", "skill-a", "install staging");
    write_skill(&root.0, ".migrating-skill-a", "skill-a", "migrating");
    write_skill(&root.0, ".tmp-skill-a", "skill-a", "tmp");
    let loaded = load_skills_from_dir(&root.0).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].metadata.name, "skill-a");
    assert!(loaded[0].content.contains("active"));
}

#[test]
fn import_install_is_visible_on_next_load() {
    let workspace = TempDir::new("install-ws");
    let source = TempDir::new("install-src");
    write_skill(&source.0, "imported-skill", "imported-skill", "from import");
    let mut config = config_with_workspace(&workspace.0, true);
    let store = resolve_configured_skills_dir(&config);
    let before = skills_generation();
    let count = import_skills_from_dir(&source.0, &store, true).unwrap();
    assert_eq!(count, 1);
    assert!(skills_generation() > before);
    config.skills.open_skills_enabled = true;
    let names: Vec<_> = load_enabled_skills(&config)
        .into_iter()
        .map(|skill| skill.metadata.name)
        .collect();
    assert!(names.contains(&"imported-skill".to_string()));
}

#[test]
fn remove_hides_skill_from_next_runtime_load() {
    let workspace = TempDir::new("remove-ws");
    let store = workspace.0.join("skills");
    write_skill(&store, "demo-skill", "demo-skill", "live");
    write_skill(
        &store,
        "demo-skill.omninova-backup",
        "demo-skill",
        "backup",
    );
    let before = skills_generation();
    skillhub_remove(&store, "demo-skill").unwrap();
    assert!(skills_generation() > before);
    let loaded = load_skills_from_dir(&store).unwrap();
    assert!(loaded.is_empty());
}

#[test]
fn rollback_loads_only_the_restored_active_version() {
    let store = TempDir::new("rollback");
    write_skill(&store.0, "demo-skill", "demo-skill", "NEW version");
    write_skill(
        &store.0,
        "demo-skill.omninova-backup",
        "demo-skill",
        "OLD version",
    );
    let before = skills_generation();
    skillhub_rollback(&store.0, "demo-skill").unwrap();
    assert!(skills_generation() > before);
    let loaded = load_skills_from_dir(&store.0).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].content.contains("OLD version"));
    assert!(!loaded.iter().any(|skill| skill.content.contains("NEW version")));
}

#[test]
fn ui_and_runtime_share_configured_skills_directory() {
    let workspace = TempDir::new("same-dir");
    let config = config_with_workspace(&workspace.0, true);
    let install_dir = resolve_configured_skills_dir(&config);
    let runtime_dir = resolve_configured_skills_dir(&config);
    assert_eq!(install_dir, runtime_dir);
    assert_eq!(install_dir, workspace.0.join("skills"));
}

#[test]
fn global_workspace_skills_dir_is_workspace_skills() {
    let workspace = TempDir::new("global-ws");
    let config = config_with_workspace(&workspace.0, true);
    assert_eq!(
        resolve_configured_skills_dir(&config),
        workspace.0.join("skills")
    );
}

#[test]
fn per_agent_workspace_does_not_silently_split_the_skill_store() {
    let global = TempDir::new("global");
    let agent_ws = TempDir::new("agent");
    let mut config = config_with_workspace(&global.0, true);
    write_skill(&global.0.join("skills"), "shared", "shared", "from global store");
    write_skill(&agent_ws.0.join("skills"), "local-only", "local-only", "agent ws");

    let effective = resolve_effective_workspace_dir(None, Some(&agent_ws.0), &config.workspace_dir)
        .expect("effective workspace");
    let implied_effective = resolve_effective_workspace_skills_dir(&config, &effective);
    let canonical = resolve_configured_skills_dir(&config);
    assert_ne!(implied_effective, canonical);

    let runtime = load_enabled_skills(&config);
    assert!(runtime.iter().any(|skill| skill.metadata.name == "shared"));
    assert!(!runtime.iter().any(|skill| skill.metadata.name == "local-only"));

    config.skills.open_skills_dir = Some(canonical.to_string_lossy().into_owned());
    assert_eq!(
        resolve_configured_skills_dir(&config),
        resolve_effective_workspace_skills_dir(&config, &effective)
    );
}

#[test]
fn enabled_false_installs_but_does_not_inject() {
    let workspace = TempDir::new("disabled");
    write_skill(
        &workspace.0.join("skills"),
        "kept-on-disk",
        "kept-on-disk",
        "should not inject",
    );
    let config = config_with_workspace(&workspace.0, false);
    let snapshot = skill_runtime_snapshot(&config);
    assert!(!snapshot.open_skills_enabled);
    assert!(snapshot.installed_slugs.contains(&"kept-on-disk".to_string()));
    assert!(snapshot.runtime_visible_slugs.is_empty());
    let mut prompt = Some("base".to_string());
    assert_eq!(inject_enabled_skills_prompt(&mut prompt, &config), 0);
    assert_eq!(prompt.as_deref(), Some("base"));
}

#[test]
fn enabled_true_injects_installed_skill() {
    let workspace = TempDir::new("enabled");
    write_skill(
        &workspace.0.join("skills"),
        "visible",
        "visible",
        "inject me",
    );
    let config = config_with_workspace(&workspace.0, true);
    let snapshot = skill_runtime_snapshot(&config);
    assert!(snapshot.open_skills_enabled);
    assert!(snapshot.runtime_visible_slugs.contains(&"visible".to_string()));
    let mut prompt = Some("base".to_string());
    assert_eq!(inject_enabled_skills_prompt(&mut prompt, &config), 1);
    assert!(prompt.as_ref().unwrap().contains("visible"));
    assert!(
        !prompt.as_ref().unwrap().contains("inject me"),
        "S2 catalog injection must not dump full SKILL.md bodies"
    );
}

#[test]
fn generation_changes_after_import_remove_rollback_and_identity_change() {
    let workspace = TempDir::new("gen");
    let source = TempDir::new("gen-src");
    write_skill(&source.0, "g-skill", "g-skill", "v1");
    let store = workspace.0.join("skills");
    let g0 = skills_generation();
    import_skills_from_dir(&source.0, &store, true).unwrap();
    let g1 = skills_generation();
    assert!(g1 > g0);

    write_skill(&store, "g-skill.omninova-backup", "g-skill", "v0");
    skillhub_rollback(&store, "g-skill").unwrap();
    let g2 = skills_generation();
    assert!(g2 > g1);

    skillhub_remove(&store, "g-skill").unwrap();
    let g3 = skills_generation();
    assert!(g3 > g2);
}

#[test]
fn long_lived_runtime_sees_refreshed_skills_after_generation_change() {
    let workspace = TempDir::new("long-lived");
    write_skill(&workspace.0.join("skills"), "first", "first", "first body");
    let config = config_with_workspace(&workspace.0, true);
    let mut prompt = Some(String::new());
    inject_enabled_skills_prompt(&mut prompt, &config);
    let seen = skills_generation();
    assert!(prompt.as_ref().unwrap().contains("first"));
    assert!(
        !prompt.as_ref().unwrap().contains("first body"),
        "catalog-only injection must omit full skill bodies"
    );
    assert!(!prompt.as_ref().unwrap().contains("second body"));

    write_skill(&workspace.0.join("skills"), "second", "second", "second body");
    bump_skills_generation();
    assert!(skills_generation() > seen);
    let mut refreshed = Some(String::new());
    inject_enabled_skills_prompt(&mut refreshed, &config);
    assert!(refreshed.as_ref().unwrap().contains("first"));
    assert!(refreshed.as_ref().unwrap().contains("second"));
    assert!(!refreshed.as_ref().unwrap().contains("first body"));
    assert!(!refreshed.as_ref().unwrap().contains("second body"));
}

#[test]
fn windows_and_unix_path_joins_stay_inside_the_configured_store() {
    let mut config = Config::default();
    config.workspace_dir = PathBuf::from("E:/novalclaw-workspace");
    let dir = resolve_configured_skills_dir(&config);
    assert!(dir.ends_with("skills"));
    assert_eq!(dir, config.workspace_dir.join("skills"));

    config.skills.open_skills_dir = Some(r"E:\omninova-skills".to_string());
    let custom = resolve_configured_skills_dir(&config);
    assert_eq!(custom, PathBuf::from(r"E:\omninova-skills"));
}

#[tokio::test]
async fn config_mutation_bumps_generation() {
    let workspace = TempDir::new("cfg-mut");
    let mut config = config_with_workspace(&workspace.0, true);
    let runtime = crate::gateway::GatewayRuntime::new(config.clone());
    let before = skills_generation();
    config.skills.open_skills_enabled = false;
    runtime.set_config(config).await.unwrap();
    assert!(skills_generation() > before);
}

#[test]
fn relative_backup_path_is_not_a_runtime_skill_directory() {
    assert!(!is_runtime_skill_directory(Path::new("skill-a.omninova-backup")));
    assert!(!is_runtime_skill_directory(Path::new(".omninova-install-x")));
    assert!(is_runtime_skill_directory(Path::new("skill-a")));
    assert!(is_runtime_skill_directory(Path::new("skill-a/references")));
}
