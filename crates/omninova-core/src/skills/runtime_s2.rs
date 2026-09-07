use super::*;
use crate::config::{resolve_configured_skills_dir, Config};
use crate::providers::ChatMessage;
use crate::skills::activation::{
    activate_skill, apply_skill_runtime_prompt, invocations_from_inbound_metadata,
    parse_skill_invocations, resolve_skill_from_store, sanitize_skill_working_context,
    SkillInvocation,
};
use crate::skills::catalog::{
    cached_skill_catalog, catalog_prompt_section, catalog_prompt_section_with_limits,
    invalidate_skill_catalog_cache, is_safe_skill_locator, search_skill_catalog,
    DEFAULT_CATALOG_DESCRIPTION_LIMIT, DEFAULT_CATALOG_PROMPT_LIMIT, MAX_ACTIVE_SKILLS,
};
use crate::tools::{SkillActivationGate, Tool, UseSkillTool};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("omninova-s2-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_skill(dir: &Path, slug: &str, name: &str, description: &str, body: &str) {
    let root = dir.join(slug);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\nversion: 1.0.0\n---\n{body}\n"),
    )
    .unwrap();
    // Mirrors the real install paths: the catalog cache keys on this, so a
    // fixture that skipped it could read a stale catalog.
    bump_skills_generation();
}

fn write_skill_with_resources(dir: &Path, slug: &str, name: &str, body: &str) {
    write_skill(dir, slug, name, "资源技能", body);
    let root = dir.join(slug);
    fs::create_dir_all(root.join("references")).unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::create_dir_all(root.join("templates")).unwrap();
    fs::write(root.join("references").join("playbook.md"), "SECRET_REFERENCE_BODY").unwrap();
    fs::write(root.join("scripts").join("run.py"), "print('should not execute')").unwrap();
    fs::write(root.join("templates").join("report.md"), "SECRET_TEMPLATE_BODY").unwrap();
}

fn config_with_workspace(workspace: &Path, enabled: bool) -> Config {
    let mut config = Config::default();
    config.workspace_dir = workspace.to_path_buf();
    config.skills.open_skills_enabled = enabled;
    config
}

fn prompt_for(config: &Config, invocations: &[SkillInvocation]) -> String {
    let mut prompt = Some("base".to_string());
    apply_skill_runtime_prompt(&mut prompt, config, invocations);
    prompt.unwrap_or_default()
}

#[test]
fn a_install_is_immediately_visible_in_catalog() {
    let workspace = TempDir::new("a-install");
    let source = TempDir::new("a-src");
    write_skill(&source.0, "baichen-legal", "Baichen Legal", "百宸律师事务所法律 AI 助手", "FULL_BAICHEN_BODY");
    let config = config_with_workspace(&workspace.0, true);
    let store = resolve_configured_skills_dir(&config);
    import_skills_from_dir(&source.0, &store, true).unwrap();

    let catalog = list_skill_catalog(&config);
    assert!(catalog.entries.iter().any(|entry| entry.id == "skill:baichen-legal"));
    assert!(catalog.entries.iter().any(|entry| entry.slug == "baichen-legal"));
}

#[test]
fn b_install_is_immediately_visible_in_slash_palette() {
    let workspace = TempDir::new("b-slash");
    let source = TempDir::new("b-src");
    write_skill(&source.0, "baichen-legal", "Baichen Legal", "百宸律师事务所法律 AI 助手", "FULL_BAICHEN_BODY");
    let config = config_with_workspace(&workspace.0, true);
    import_skills_from_dir(&source.0, &resolve_configured_skills_dir(&config), true).unwrap();

    let palette = list_command_palette(&config);
    assert!(palette.skills.iter().any(|item| item.id == "skill:baichen-legal"));
    assert!(palette.skills.iter().any(|item| item.command_alias == "/baichen-legal"));
}

#[test]
fn c_install_can_activate_without_restart() {
    let workspace = TempDir::new("c-activate");
    let source = TempDir::new("c-src");
    write_skill(&source.0, "baichen-legal", "Baichen Legal", "百宸律师事务所法律 AI 助手", "FULL_BAICHEN_BODY");
    let config = config_with_workspace(&workspace.0, true);
    import_skills_from_dir(&source.0, &resolve_configured_skills_dir(&config), true).unwrap();

    let activated = activate_skill(&config, "skill:baichen-legal", "slash_command").unwrap();
    assert_eq!(activated.skill_id, "skill:baichen-legal");
    assert!(activated.instructions.contains("FULL_BAICHEN_BODY"));
    assert_eq!(activated.selection_source, "explicit_slash");
}

#[test]
fn d_generation_bump_means_no_restart_required() {
    let workspace = TempDir::new("d-gen");
    let config = config_with_workspace(&workspace.0, true);
    let before = list_skill_catalog(&config).generation;
    write_skill(&resolve_configured_skills_dir(&config), "fresh", "fresh", "新技能", "FRESH_BODY");
    bump_skills_generation();
    let after = list_skill_catalog(&config);
    assert!(after.generation > before);
    assert!(after.entries.iter().any(|entry| entry.id == "skill:fresh"));
}

#[test]
fn e_slash_parser_and_command_token_open_palette_model() {
    assert!(parse_slash_command("/").is_some());
    assert_eq!(
        command_token_at("/", 1).map(|item| item.2),
        Some("/".to_string())
    );
    assert_eq!(
        command_token_at("hello /leg", 10).map(|item| item.2),
        Some("/leg".to_string())
    );
    let parsed = parse_slash_command("/baichen-legal 帮我审查这份合同").unwrap();
    match parsed {
        ParsedSlashCommand::Skill { id, rest } => {
            assert_eq!(id, "skill:baichen-legal");
            assert_eq!(rest, "帮我审查这份合同");
        }
        other => panic!("expected skill parse, got {other:?}"),
    }
}

#[test]
fn f_typing_filters_name_slug_and_chinese_description() {
    let workspace = TempDir::new("f-filter");
    write_skill(
        &workspace.0.join("skills"),
        "baichen-legal",
        "Baichen Legal",
        "百宸律师事务所法律 AI 助手",
        "BODY",
    );
    write_skill(
        &workspace.0.join("skills"),
        "pdf-tools",
        "PDF",
        "Read/create/render PDFs",
        "PDF_BODY",
    );
    let palette = list_command_palette(&config_with_workspace(&workspace.0, true));
    let by_name = filter_command_palette(&palette, "/baichen");
    assert_eq!(by_name.skills.len(), 1);
    assert_eq!(by_name.skills[0].id, "skill:baichen-legal");
    let by_zh = filter_command_palette(&palette, "/合同");
    assert!(by_zh.skills.is_empty(), "description has 法律 not 合同");
    write_skill(
        &workspace.0.join("skills"),
        "legal-contract-review",
        "Legal Contract Review",
        "审查合同风险与条款",
        "CONTRACT_BODY",
    );
    let refreshed = list_command_palette(&config_with_workspace(&workspace.0, true));
    let matched = filter_command_palette(&refreshed, "/合同");
    assert!(matched.skills.iter().any(|item| item.id == "skill:legal-contract-review"));
    let system_only = filter_command_palette(&refreshed, "/help");
    assert!(system_only.system.iter().any(|item| item.id == "system:help"));
}

#[test]
fn g_palette_rows_are_stable_for_arrow_navigation() {
    let palette = list_command_palette(&Config::default());
    assert!(!palette.system.is_empty());
    let rows: Vec<_> = palette.system.iter().chain(palette.skills.iter()).collect();
    assert!(rows.len() >= 2, "system commands must remain arrow-navigable");
    let last = rows.len() - 1;
    assert_eq!((0 + last) % rows.len(), last);
    assert_eq!((last + 1) % rows.len(), 0);
    assert_eq!(rows[0].id, "system:help");
}

#[test]
fn h_enter_selects_skill_id_not_display_name() {
    let parsed = parse_slash_command("/skill baichen-legal").unwrap();
    match parsed {
        ParsedSlashCommand::Skill { id, rest } => {
            assert_eq!(id, "skill:baichen-legal");
            assert!(rest.is_empty());
            assert_ne!(id, "Baichen Legal");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn i_escape_keeps_current_input_token() {
    let token = command_token_at("/leg review", 4).unwrap();
    assert_eq!(token.2, "/leg");
    assert_eq!(&"/leg review"[token.0..token.1], "/leg");
}

#[test]
fn j_explicit_selected_skill_only() {
    let workspace = TempDir::new("j-explicit");
    write_skill(&workspace.0.join("skills"), "alpha", "Alpha", "alpha desc", "ALPHA_ONLY_BODY");
    write_skill(&workspace.0.join("skills"), "beta", "Beta", "beta desc", "BETA_ONLY_BODY");
    let config = config_with_workspace(&workspace.0, true);
    let prompt = prompt_for(
        &config,
        &[SkillInvocation {
            skill_id: "skill:alpha".into(),
            source: "slash_command".into(),
        }],
    );
    assert!(prompt.contains("ALPHA_ONLY_BODY"));
    assert!(!prompt.contains("BETA_ONLY_BODY"));
}

#[test]
fn k_other_skills_absent_from_full_prompt() {
    let workspace = TempDir::new("k-absent");
    write_skill(&workspace.0.join("skills"), "one", "One", "one desc", "ONE_FULL_INSTRUCTIONS");
    write_skill(&workspace.0.join("skills"), "two", "Two", "two desc", "TWO_FULL_INSTRUCTIONS");
    let config = config_with_workspace(&workspace.0, true);
    let prompt = prompt_for(
        &config,
        &[SkillInvocation {
            skill_id: "skill:one".into(),
            source: "explicit_slash".into(),
        }],
    );
    assert!(prompt.contains("skill:two") || prompt.contains("/two") || prompt.contains("Two"));
    assert!(!prompt.contains("TWO_FULL_INSTRUCTIONS"));
}

#[test]
fn l_auto_mode_injects_catalog_only() {
    let workspace = TempDir::new("l-auto");
    write_skill(&workspace.0.join("skills"), "one", "One", "one desc", "ONE_FULL_INSTRUCTIONS");
    write_skill(&workspace.0.join("skills"), "two", "Two", "two desc", "TWO_FULL_INSTRUCTIONS");
    let config = config_with_workspace(&workspace.0, true);
    let prompt = prompt_for(&config, &[]);
    assert!(prompt.contains("Skill Catalog") || prompt.contains("skill:one"));
    assert!(!prompt.contains("ONE_FULL_INSTRUCTIONS"));
    assert!(!prompt.contains("TWO_FULL_INSTRUCTIONS"));
    assert!(prompt.contains("use_skill"));
}

#[tokio::test]
async fn m_use_skill_loads_actual_selected_instructions() {
    let workspace = TempDir::new("m-use");
    write_skill(&workspace.0.join("skills"), "one", "One", "one desc", "ONE_FULL_INSTRUCTIONS");
    let config = config_with_workspace(&workspace.0, true);
    let tool = UseSkillTool::new(config, Arc::new(SkillActivationGate::with_explicit(None)));
    let result = tool
        .execute(json!({ "skill_id": "skill:one" }))
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.output.contains("ONE_FULL_INSTRUCTIONS"));
    let message = activation_system_message("use_skill", &result.output).unwrap();
    assert!(message.contains("ONE_FULL_INSTRUCTIONS"));
    assert!(message.contains("skill:one"));
}

#[test]
fn n_unknown_skill_is_rejected() {
    let workspace = TempDir::new("n-unknown");
    let config = config_with_workspace(&workspace.0, true);
    let err = activate_skill(&config, "skill:missing", "slash_command").unwrap_err();
    assert!(err.contains("unknown") || err.contains("unavailable") || err.contains("关闭"));
}

#[test]
fn o_remove_disappears_immediately() {
    let workspace = TempDir::new("o-remove");
    let store = workspace.0.join("skills");
    write_skill(&store, "demo-skill", "Demo", "demo desc", "LIVE_BODY");
    let config = config_with_workspace(&workspace.0, true);
    assert!(list_skill_catalog(&config)
        .entries
        .iter()
        .any(|entry| entry.id == "skill:demo-skill"));
    skillhub_remove(&store, "demo-skill").unwrap();
    let catalog = list_skill_catalog(&config);
    assert!(!catalog.entries.iter().any(|entry| entry.id == "skill:demo-skill"));
    let palette = list_command_palette(&config);
    assert!(!palette.skills.iter().any(|item| item.id == "skill:demo-skill"));
    assert!(activate_skill(&config, "skill:demo-skill", "auto_use_skill").is_err());
}

#[test]
fn p_rollback_updates_catalog_and_activation() {
    let store = TempDir::new("p-rollback");
    write_skill(&store.0, "demo-skill", "Demo", "new desc", "NEW_VERSION_BODY");
    write_skill(
        &store.0,
        "demo-skill.omninova-backup",
        "Demo",
        "old desc",
        "OLD_VERSION_BODY",
    );
    let mut config = Config::default();
    config.workspace_dir = store.0.clone();
    config.skills.open_skills_dir = Some(store.0.to_string_lossy().into_owned());
    config.skills.open_skills_enabled = true;
    skillhub_rollback(&store.0, "demo-skill").unwrap();
    let catalog = list_skill_catalog(&config);
    assert_eq!(
        catalog
            .entries
            .iter()
            .filter(|entry| entry.slug == "demo-skill")
            .count(),
        1
    );
    let activated = activate_skill(&config, "skill:demo-skill", "slash_command").unwrap();
    assert!(activated.instructions.contains("OLD_VERSION_BODY"));
    assert!(!activated.instructions.contains("NEW_VERSION_BODY"));
}

#[test]
fn q_backup_and_staging_absent_from_catalog() {
    let workspace = TempDir::new("q-backup");
    let store = workspace.0.join("skills");
    write_skill(&store, "live", "Live", "live desc", "LIVE");
    write_skill(&store, "live.omninova-backup", "Live", "backup desc", "BACKUP_BODY");
    write_skill(&store, ".omninova-install-live-1-1", "Live", "stage desc", "STAGE_BODY");
    write_skill(&store, ".omninova-rollback-live-1", "Live", "roll desc", "ROLL_BODY");
    let catalog = list_skill_catalog(&config_with_workspace(&workspace.0, true));
    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(catalog.entries[0].id, "skill:live");
    let prompt = catalog_prompt_section(&catalog);
    assert!(!prompt.contains("BACKUP_BODY"));
    assert!(!prompt.contains("STAGE_BODY"));
}

#[test]
fn r_disabled_state_hides_skills_from_runtime() {
    let workspace = TempDir::new("r-disabled");
    write_skill(&workspace.0.join("skills"), "kept", "Kept", "kept desc", "KEPT_BODY");
    let config = config_with_workspace(&workspace.0, false);
    let palette = list_command_palette(&config);
    assert!(palette.system.iter().any(|item| item.id == "system:help"));
    assert!(palette.skills.is_empty());
    assert_eq!(palette.skills_empty_reason.as_deref(), Some("技能功能已关闭"));
    let prompt = prompt_for(&config, &[]);
    assert!(!prompt.contains("KEPT_BODY"));
    let err = resolve_skill_from_store(&config, "skill:kept").unwrap_err();
    assert!(err.contains("技能功能已关闭"));
}

#[test]
fn s_normal_chat_does_not_require_a_skill() {
    let workspace = TempDir::new("s-chat");
    write_skill(&workspace.0.join("skills"), "one", "One", "one desc", "ONE_FULL_INSTRUCTIONS");
    let config = config_with_workspace(&workspace.0, true);
    let result = apply_skill_runtime_prompt(&mut Some("hello".into()), &config, &[]);
    assert!(result.activated.is_empty());
    assert_eq!(MAX_ACTIVE_SKILLS, 1);
}

#[test]
fn t_gateway_request_dto_is_backward_compatible() {
    assert!(parse_skill_invocations(None).is_empty());
    assert!(parse_skill_invocations(Some(&json!([]))).is_empty());
    let camel = parse_skill_invocations(Some(&json!([{
        "skillId": "skill:baichen-legal",
        "source": "slash_command"
    }])));
    assert_eq!(camel[0].skill_id, "skill:baichen-legal");
    let mut metadata = HashMap::new();
    metadata.insert(
        "skillInvocations".to_string(),
        json!([{ "skillId": "skill:one", "source": "slash_command" }]),
    );
    let from_meta = invocations_from_inbound_metadata(&metadata);
    assert_eq!(from_meta[0].skill_id, "skill:one");
    let encoded = serde_json::to_value(&from_meta).unwrap();
    assert_eq!(encoded[0]["skillId"], "skill:one");
    let too_many = parse_skill_invocations(Some(&json!([
        { "skillId": "skill:one", "source": "slash_command" },
        { "skillId": "skill:two", "source": "slash_command" }
    ])));
    assert_eq!(too_many.len(), 1);
}

#[test]
fn explicit_activation_reports_unavailable_skill_instead_of_silent_fallback() {
    let workspace = TempDir::new("explicit-unavailable");
    let config = config_with_workspace(&workspace.0, true);
    let mut prompt = Some("base".to_string());
    let result = apply_skill_runtime_prompt(
        &mut prompt,
        &config,
        &[SkillInvocation {
            skill_id: "skill:missing".into(),
            source: "slash_command".into(),
        }],
    );
    assert!(result.activated.is_empty());
    assert_eq!(result.errors.len(), 1);
    assert!(result.errors[0].contains("skill:missing"));
}

#[test]
fn u_ten_skills_inflate_metadata_not_full_instructions() {
    let workspace = TempDir::new("u-tokens");
    let store = workspace.0.join("skills");
    let large_body = "FULL_INSTRUCTION_BLOCK_".repeat(120);
    for index in 0..10 {
        write_skill(
            &store,
            &format!("skill-{index:02}"),
            &format!("Skill {index:02}"),
            &format!("short catalog desc {index:02}"),
            &large_body,
        );
    }
    let config = config_with_workspace(&workspace.0, true);
    let empty_workspace = TempDir::new("u-empty");
    let empty_config = config_with_workspace(&empty_workspace.0, true);
    let tokens_with_0 = estimate_prompt_tokens(&prompt_for(&empty_config, &[]));
    let catalog_prompt = prompt_for(&config, &[]);
    let tokens_with_10 = estimate_prompt_tokens(&catalog_prompt);
    let loaded = load_enabled_skills(&config);
    let full_dump = format_skills_prompt(&loaded);
    let dump_tokens = estimate_prompt_tokens(&full_dump);
    assert!(
        !catalog_prompt.contains("FULL_INSTRUCTION_BLOCK_"),
        "catalog prompt must not include full skill bodies"
    );
    assert!(
        tokens_with_10 > tokens_with_0,
        "TOKENS_WITH_0_SKILLS={tokens_with_0} TOKENS_WITH_10_SKILLS_CATALOG={tokens_with_10}"
    );
    let growth = tokens_with_10.saturating_sub(tokens_with_0);
    eprintln!(
        "TOKENS_WITH_0_SKILLS={tokens_with_0} TOKENS_WITH_10_SKILLS_CATALOG={tokens_with_10} FULL_DUMP={dump_tokens} growth={growth}"
    );
    assert!(
        growth * 3 < dump_tokens,
        "TOKENS_WITH_0_SKILLS={tokens_with_0} TOKENS_WITH_10_SKILLS_CATALOG={tokens_with_10} FULL_DUMP={dump_tokens} growth={growth}"
    );
}

#[test]
fn v_windows_paths_stay_inside_configured_store() {
    let mut config = Config::default();
    config.workspace_dir = PathBuf::from(r"E:\novalclaw-workspace");
    let dir = resolve_configured_skills_dir(&config);
    assert_eq!(dir, PathBuf::from(r"E:\novalclaw-workspace").join("skills"));
    config.skills.open_skills_dir = Some(r"E:\omninova-skills".to_string());
    assert_eq!(
        resolve_configured_skills_dir(&config),
        PathBuf::from(r"E:\omninova-skills")
    );
    assert!(!is_safe_skill_locator(r"E:\omninova-skills\secret"));
    assert!(!is_safe_skill_locator(r"..\..\windows\system32"));
}

#[test]
fn w_malicious_skill_id_and_path_traversal_rejected() {
    let workspace = TempDir::new("w-jail");
    write_skill(&workspace.0.join("skills"), "safe", "Safe", "safe desc", "SAFE_BODY");
    let config = config_with_workspace(&workspace.0, true);
    for locator in [
        "../../../etc/passwd",
        r"..\..\secret",
        "/etc/passwd",
        r"E:\outside\SKILL.md",
        "skill:../escape",
        "skill:..\\escape",
        "skill:safe/../../etc",
    ] {
        assert!(
            !is_safe_skill_locator(locator) || resolve_skill_from_store(&config, locator).is_err(),
            "locator should be rejected: {locator}"
        );
        assert!(activate_skill(&config, locator, "auto_use_skill").is_err());
    }
}

#[test]
fn resource_index_exposes_names_not_file_bodies() {
    let workspace = TempDir::new("resources");
    write_skill_with_resources(&workspace.0.join("skills"), "legal", "Legal", "LEGAL_BODY");
    let catalog = list_skill_catalog(&config_with_workspace(&workspace.0, true));
    let entry = catalog.get("skill:legal").unwrap();
    assert!(entry.has_scripts);
    assert!(entry.resources.iter().any(|item| item.name == "playbook.md"));
    assert!(entry.resources.iter().any(|item| item.kind == "script"));
    let catalog_prompt = catalog_prompt_section(&catalog);
    assert!(!catalog_prompt.contains("SECRET_REFERENCE_BODY"));
    assert!(!catalog_prompt.contains("print('should not execute')"));
    let activated = activate_skill(&config_with_workspace(&workspace.0, true), "skill:legal", "auto_use_skill")
        .unwrap();
    assert!(activated.resource_prompt.contains("playbook.md"));
    assert!(activated.resource_prompt.contains("must not be executed"));
    assert!(!activated.resource_prompt.contains("SECRET_REFERENCE_BODY"));
}

#[test]
fn empty_catalog_keeps_system_commands() {
    let workspace = TempDir::new("empty");
    let palette = list_command_palette(&config_with_workspace(&workspace.0, true));
    assert!(palette.system.iter().any(|item| item.id == "system:help"));
    assert!(palette.system.iter().any(|item| item.id == "system:skills"));
    assert_eq!(palette.skills_empty_reason.as_deref(), Some("暂无可用技能"));
}

#[tokio::test]
async fn use_skill_gate_rejects_second_skill_when_explicit_is_set() {
    let workspace = TempDir::new("gate");
    write_skill(&workspace.0.join("skills"), "one", "One", "one desc", "ONE_BODY");
    write_skill(&workspace.0.join("skills"), "two", "Two", "two desc", "TWO_BODY");
    let config = config_with_workspace(&workspace.0, true);
    let tool = UseSkillTool::new(
        config,
        Arc::new(SkillActivationGate::with_explicit(Some("skill:one".into()))),
    );
    let blocked = tool
        .execute(json!({ "skill_id": "skill:two" }))
        .await
        .unwrap();
    assert!(!blocked.success);
}

#[test]
fn explicit_skill_is_request_scoped_and_absent_next_turn() {
    let workspace = TempDir::new("s21-explicit-scope");
    write_skill(
        &workspace.0.join("skills"),
        "alpha",
        "Alpha",
        "alpha desc",
        "ALPHA_FULL_INSTRUCTIONS",
    );
    let config = config_with_workspace(&workspace.0, true);
    let turn1 = prompt_for(
        &config,
        &[SkillInvocation {
            skill_id: "skill:alpha".into(),
            source: "slash_command".into(),
        }],
    );
    assert!(turn1.contains("ALPHA_FULL_INSTRUCTIONS"));

    let persisted = sanitize_skill_working_context(vec![
        ChatMessage::system(turn1),
        ChatMessage::user("review this"),
        ChatMessage::assistant("done"),
    ]);
    let persisted_text: String = persisted.iter().map(|message| message.content.clone()).collect();
    assert!(!persisted_text.contains("ALPHA_FULL_INSTRUCTIONS"));

    let turn2 = prompt_for(&config, &[]);
    assert!(!turn2.contains("ALPHA_FULL_INSTRUCTIONS"));
    assert!(turn2.contains("skill:alpha") || turn2.contains("Skill Catalog"));
}

#[tokio::test]
async fn auto_use_skill_is_request_scoped_and_absent_next_turn() {
    let workspace = TempDir::new("s21-auto-scope");
    write_skill(
        &workspace.0.join("skills"),
        "alpha",
        "Alpha",
        "alpha desc",
        "ALPHA_FULL_INSTRUCTIONS",
    );
    let config = config_with_workspace(&workspace.0, true);
    let tool = UseSkillTool::new(
        config.clone(),
        Arc::new(SkillActivationGate::with_explicit(None)),
    );
    let result = tool
        .execute(json!({ "skill_id": "skill:alpha" }))
        .await
        .unwrap();
    let activation = activation_system_message("use_skill", &result.output).unwrap();
    assert!(activation.contains("ALPHA_FULL_INSTRUCTIONS"));

    let persisted = sanitize_skill_working_context(vec![
        ChatMessage::system(prompt_for(&config, &[])),
        ChatMessage::user("do the task"),
        ChatMessage::assistant(
            r#"{"content":null,"tool_calls":[{"id":"call_1","name":"use_skill","arguments":"{}"}]}"#,
        ),
        ChatMessage::tool(
            json!({
                "tool_call_id": "call_1",
                "content": result.output,
            })
            .to_string(),
        ),
        ChatMessage::system(activation),
        ChatMessage::assistant("finished"),
    ]);
    let persisted_text: String = persisted.iter().map(|message| message.content.clone()).collect();
    assert!(!persisted_text.contains("ALPHA_FULL_INSTRUCTIONS"));
    assert!(persisted.iter().all(|message| {
        message.role != "system" || !message.content.contains("## Active Skill")
    }));
}

#[test]
fn transcript_does_not_keep_skill_system_messages() {
    let messages = vec![
        ChatMessage::system("catalog only"),
        ChatMessage::user("hello"),
        ChatMessage::system("## Active Skill\n\nSECRET_SKILL_BODY"),
        ChatMessage::assistant("ok"),
    ];
    let cleaned = sanitize_skill_working_context(messages);
    assert_eq!(cleaned.len(), 3);
    assert!(cleaned
        .iter()
        .all(|message| !message.content.contains("SECRET_SKILL_BODY")));
    assert_eq!(cleaned[0].role, "system");
    assert_eq!(cleaned[1].role, "user");
    assert_eq!(cleaned[2].role, "assistant");
}

#[tokio::test]
async fn duplicate_use_skill_does_not_reappend_instructions() {
    let workspace = TempDir::new("s21-dup");
    write_skill(
        &workspace.0.join("skills"),
        "alpha",
        "Alpha",
        "alpha desc",
        "ALPHA_FULL_INSTRUCTIONS",
    );
    let config = config_with_workspace(&workspace.0, true);
    let tool = UseSkillTool::new(config, Arc::new(SkillActivationGate::with_explicit(None)));
    let first = tool
        .execute(json!({ "skill_id": "skill:alpha" }))
        .await
        .unwrap();
    let second = tool
        .execute(json!({ "skill_id": "skill:alpha" }))
        .await
        .unwrap();
    assert!(first.success);
    assert!(second.success);
    assert!(first.output.contains("ALPHA_FULL_INSTRUCTIONS"));
    assert!(!second.output.contains("ALPHA_FULL_INSTRUCTIONS"));
    assert!(second.output.contains("already_active"));
    assert!(activation_system_message("use_skill", &first.output).is_some());
    assert!(activation_system_message("use_skill", &second.output).is_none());
}

#[tokio::test]
async fn explicit_selection_blocks_auto_switch() {
    let workspace = TempDir::new("s21-prec");
    write_skill(
        &workspace.0.join("skills"),
        "alpha",
        "Alpha",
        "alpha desc",
        "ALPHA_BODY",
    );
    write_skill(&workspace.0.join("skills"), "beta", "Beta", "beta desc", "BETA_BODY");
    let config = config_with_workspace(&workspace.0, true);
    let tool = UseSkillTool::new(
        config,
        Arc::new(SkillActivationGate::with_explicit(Some("skill:alpha".into()))),
    );
    let same = tool
        .execute(json!({ "skill_id": "skill:alpha" }))
        .await
        .unwrap();
    assert!(same.success);
    assert!(same.output.contains("already_active"));
    assert!(!same.output.contains("ALPHA_BODY"));
    let other = tool
        .execute(json!({ "skill_id": "skill:beta" }))
        .await
        .unwrap();
    assert!(!other.success);
}

#[test]
fn stale_removed_skill_does_not_fallback() {
    let workspace = TempDir::new("s21-stale");
    write_skill(
        &workspace.0.join("skills"),
        "alpha",
        "Alpha",
        "alpha desc",
        "ALPHA_BODY",
    );
    write_skill(&workspace.0.join("skills"), "beta", "Beta", "beta desc", "BETA_BODY");
    let config = config_with_workspace(&workspace.0, true);
    skillhub_remove(&workspace.0.join("skills"), "alpha").unwrap();
    assert!(activate_skill(&config, "skill:alpha", "slash_command").is_err());
    let prompt = prompt_for(
        &config,
        &[SkillInvocation {
            skill_id: "skill:alpha".into(),
            source: "slash_command".into(),
        }],
    );
    assert!(!prompt.contains("ALPHA_BODY"));
    assert!(!prompt.contains("BETA_BODY"));
    assert!(prompt.contains("skill:beta") || prompt.contains("Beta"));
}

/// Writes `count` marketplace-shaped skills with human-length descriptions,
/// the shape that grew one real system prompt to 3.2 MB.
fn write_bulk_skills(dir: &Path, count: usize) {
    let long_description = "Comprehensive toolkit for the task with references, templates and \
scripts covering ingestion, validation, transformation, reporting and archival, plus guidance on \
when to prefer it over the built-in tools and how to recover from partial failures."
        .to_string();
    for index in 0..count {
        write_skill(
            dir,
            &format!("vendor-{index:05}"),
            &format!("Vendor Skill {index:05}"),
            &long_description,
            "BULK_BODY",
        );
    }
}

#[test]
fn oversized_catalog_is_capped_in_the_prompt() {
    let workspace = TempDir::new("s2-cap");
    write_bulk_skills(&workspace.0.join("skills"), 400);
    let config = config_with_workspace(&workspace.0, true);
    let catalog = list_skill_catalog(&config);
    assert_eq!(catalog.entries.len(), 400);

    let capped = catalog_prompt_section_with_limits(&catalog, 150, 160);
    let listed = capped.matches("- `skill:vendor-").count();
    assert_eq!(listed, 150, "only the cap may be inlined");
    assert!(
        capped.contains("250 more installed skills are not listed above"),
        "the model must be told the listing is partial: {capped:.400}"
    );
    assert!(capped.contains("use_skill` with a `query`"));

    let uncapped = catalog_prompt_section_with_limits(&catalog, 0, 0);
    assert_eq!(uncapped.matches("- `skill:vendor-").count(), 400);
    assert!(
        capped.len() < uncapped.len(),
        "capping must shrink the prompt: {} vs {}",
        capped.len(),
        uncapped.len()
    );

    // The property that actually matters: the prompt stops tracking catalog
    // size, so a mirrored marketplace cannot inflate it without bound.
    let bigger = TempDir::new("s2-cap-grown");
    write_bulk_skills(&bigger.0.join("skills"), 1200);
    let grown_config = config_with_workspace(&bigger.0, true);
    let grown = list_skill_catalog(&grown_config);
    assert_eq!(grown.entries.len(), 1200);
    let grown_prompt = catalog_prompt_section_with_limits(&grown, 150, 160);
    assert_eq!(
        grown_prompt.matches("- `skill:vendor-").count(),
        150,
        "tripling the catalog must not add listed entries"
    );
    assert!(
        grown_prompt.len() < capped.len() + 200,
        "capped prompt grew with catalog size: {} vs {}",
        grown_prompt.len(),
        capped.len()
    );
}

#[test]
fn catalog_descriptions_are_truncated_per_entry() {
    let workspace = TempDir::new("s2-desc");
    write_bulk_skills(&workspace.0.join("skills"), 2);
    let config = config_with_workspace(&workspace.0, true);
    let catalog = list_skill_catalog(&config);

    let capped = catalog_prompt_section_with_limits(&catalog, 150, 60);
    assert!(capped.contains('…'), "truncated entries need an ellipsis");
    for line in capped.lines().filter(|line| line.starts_with("- `skill:")) {
        assert!(
            line.chars().count() < 200,
            "entry line stayed long after truncation: {line}"
        );
    }
    assert!(!capped.contains("how to recover from partial failures"));
}

#[tokio::test]
async fn skills_beyond_the_prompt_cap_stay_reachable_by_search() {
    let workspace = TempDir::new("s2-search");
    let skills_dir = workspace.0.join("skills");
    write_bulk_skills(&skills_dir, 300);
    // Sorts last by display name, so a capped listing cannot include it.
    write_skill(
        &skills_dir,
        "zz-radiology",
        "ZZ Radiology Report Reader",
        "解读放射科影像报告并生成结构化随访建议",
        "RADIOLOGY_BODY",
    );
    let config = config_with_workspace(&workspace.0, true);
    let catalog = list_skill_catalog(&config);

    let capped = catalog_prompt_section_with_limits(&catalog, 150, 160);
    assert!(
        !capped.contains("skill:zz-radiology"),
        "fixture must fall outside the cap"
    );

    let hits = search_skill_catalog(&catalog, "radiology", 25);
    assert_eq!(hits.first().map(|entry| entry.id.as_str()), Some("skill:zz-radiology"));

    let tool = UseSkillTool::new(config, Arc::new(SkillActivationGate::default()));
    let found = tool.execute(json!({ "query": "radiology" })).await.unwrap();
    assert!(found.success);
    let payload: serde_json::Value = serde_json::from_str(&found.output).unwrap();
    assert_eq!(payload["mode"], "search");
    assert_eq!(payload["catalog_size"], 301);
    assert_eq!(payload["results"][0]["skill_id"], "skill:zz-radiology");
    // Search must not activate anything, only report candidates.
    assert!(!found.output.contains("RADIOLOGY_BODY"));

    let loaded = tool
        .execute(json!({ "skill_id": "skill:zz-radiology" }))
        .await
        .unwrap();
    assert!(loaded.success);
    assert!(loaded.output.contains("RADIOLOGY_BODY"));
}

#[tokio::test]
async fn a_guessed_skill_id_comes_back_with_candidates() {
    let workspace = TempDir::new("s2-suggest");
    write_skill(
        &workspace.0.join("skills"),
        "pdf-table-extract",
        "PDF Table Extract",
        "extract tables from pdf files",
        "PDF_BODY",
    );
    let config = config_with_workspace(&workspace.0, true);
    let tool = UseSkillTool::new(config, Arc::new(SkillActivationGate::default()));

    let missed = tool
        .execute(json!({ "skill_id": "skill:pdf_table" }))
        .await
        .unwrap();
    assert!(!missed.success);
    let payload: serde_json::Value = serde_json::from_str(&missed.output).unwrap();
    assert_eq!(payload["did_you_mean"][0], "skill:pdf-table-extract");
}

#[test]
fn catalog_prompt_defaults_are_bounded() {
    // Guards the regression that made the catalog alone exceed every
    // provider's input budget.
    assert!(DEFAULT_CATALOG_PROMPT_LIMIT > 0);
    assert!(DEFAULT_CATALOG_DESCRIPTION_LIMIT > 0);
    let defaults = Config::default();
    assert_eq!(
        defaults.skills.catalog_prompt_limit,
        DEFAULT_CATALOG_PROMPT_LIMIT
    );
    assert_eq!(
        defaults.skills.catalog_description_limit,
        DEFAULT_CATALOG_DESCRIPTION_LIMIT
    );
}

/// Cache-hit behaviour itself is covered by `catalog::cache_tests`, which
/// tests the freshness predicate directly. It cannot be asserted here: the
/// cache is one process-global slot, so a test running in parallel against a
/// different store legitimately evicts this one's entry.
#[test]
fn repeated_catalog_reads_agree() {
    let workspace = TempDir::new("s2-cache-hit");
    write_bulk_skills(&workspace.0.join("skills"), 20);
    let config = config_with_workspace(&workspace.0, true);

    let first = cached_skill_catalog(&config);
    let second = cached_skill_catalog(&config);
    assert_eq!(first.entries.len(), 20);
    assert_eq!(first.entries, second.entries);
    assert_eq!(first.skills_dir, second.skills_dir);
}

#[test]
fn installing_a_skill_invalidates_the_catalog_cache() {
    let workspace = TempDir::new("s2-cache-gen");
    let skills_dir = workspace.0.join("skills");
    write_bulk_skills(&skills_dir, 5);
    let config = config_with_workspace(&workspace.0, true);
    let before = cached_skill_catalog(&config);
    assert_eq!(before.entries.len(), 5);

    write_skill(&skills_dir, "late-arrival", "Late Arrival", "加入得比较晚", "LATE_BODY");
    let after = cached_skill_catalog(&config);
    assert!(!Arc::ptr_eq(&before, &after), "generation bump must rebuild");
    assert_eq!(after.entries.len(), 6);
    assert!(after
        .entries
        .iter()
        .any(|entry| entry.id == "skill:late-arrival"));
}

#[test]
fn explicit_invalidation_forces_a_rebuild() {
    let workspace = TempDir::new("s2-cache-drop");
    write_bulk_skills(&workspace.0.join("skills"), 3);
    let config = config_with_workspace(&workspace.0, true);

    let before = cached_skill_catalog(&config);
    invalidate_skill_catalog_cache();
    let after = cached_skill_catalog(&config);
    assert!(!Arc::ptr_eq(&before, &after));
    assert_eq!(before.entries.len(), after.entries.len());
}

#[test]
fn a_different_store_is_not_served_from_cache() {
    let first_ws = TempDir::new("s2-cache-dir-a");
    write_bulk_skills(&first_ws.0.join("skills"), 4);
    let first_config = config_with_workspace(&first_ws.0, true);
    let first = cached_skill_catalog(&first_config);
    assert_eq!(first.entries.len(), 4);

    let second_ws = TempDir::new("s2-cache-dir-b");
    write_bulk_skills(&second_ws.0.join("skills"), 9);
    let second_config = config_with_workspace(&second_ws.0, true);
    let second = cached_skill_catalog(&second_config);
    assert_eq!(second.entries.len(), 9, "must not serve another store's catalog");
    assert_eq!(second.skills_dir, second_ws.0.join("skills"));
}

#[test]
fn disabling_skills_is_not_served_from_cache() {
    let workspace = TempDir::new("s2-cache-toggle");
    write_bulk_skills(&workspace.0.join("skills"), 4);
    let enabled = config_with_workspace(&workspace.0, true);
    assert!(cached_skill_catalog(&enabled).open_skills_enabled);

    let disabled = config_with_workspace(&workspace.0, false);
    let catalog = cached_skill_catalog(&disabled);
    assert!(!catalog.open_skills_enabled);
    assert!(catalog.entries.iter().all(|entry| !entry.runtime_visible));
}

#[test]
fn max_active_skills_remains_one() {
    assert_eq!(MAX_ACTIVE_SKILLS, 1);
    let parsed = parse_skill_invocations(Some(&json!([
        { "skillId": "skill:one", "source": "slash_command" },
        { "skillId": "skill:two", "source": "slash_command" }
    ])));
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].skill_id, "skill:one");
}
