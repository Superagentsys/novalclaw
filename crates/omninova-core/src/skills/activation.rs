use super::catalog::{
    cached_skill_catalog, catalog_prompt_section_with_limits, is_safe_skill_locator,
    load_skill_instructions, normalize_skill_id, SkillCatalogEntry, MAX_ACTIVE_SKILLS,
};
use super::prompt::{parse_skill_prompt, SkillPromptValidation};
use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillInvocation {
    #[serde(default, alias = "skill_id")]
    pub skill_id: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSkillActivation {
    pub skill_id: String,
    pub display_name: String,
    pub version: String,
    pub selection_source: String,
    pub instructions: String,
    pub resource_prompt: String,
    pub source: String,
    pub raw_skill_chars: usize,
    pub active_skill_chars: usize,
    pub validation: SkillPromptValidation,
    pub provider_envelope: String,
}

#[derive(Debug, Clone, Default)]
pub struct SkillRuntimePromptResult {
    pub catalog_count: usize,
    pub activated: Vec<ResolvedSkillActivation>,
    pub errors: Vec<String>,
}

pub fn parse_skill_invocations(value: Option<&serde_json::Value>) -> Vec<SkillInvocation> {
    let Some(value) = value else {
        return Vec::new();
    };
    if let Ok(items) = serde_json::from_value::<Vec<SkillInvocation>>(value.clone()) {
        return items
            .into_iter()
            .filter(|item| !item.skill_id.trim().is_empty())
            .take(MAX_ACTIVE_SKILLS)
            .collect();
    }
    if let Some(id) = value.as_str() {
        if !id.trim().is_empty() {
            return vec![SkillInvocation {
                skill_id: id.to_string(),
                source: "explicit".into(),
            }];
        }
    }
    Vec::new()
}

pub fn resolve_skill_from_store(
    config: &Config,
    skill_id: &str,
) -> Result<SkillCatalogEntry, String> {
    if !config.skills.open_skills_enabled {
        return Err("技能功能已关闭".to_string());
    }
    if !is_safe_skill_locator(skill_id) {
        return Err("unknown skill".to_string());
    }
    let catalog = cached_skill_catalog(config);
    let Some(entry) = catalog.get(skill_id).cloned() else {
        return Err("unknown skill".to_string());
    };
    if !entry.runtime_visible {
        return Err("skill unavailable".to_string());
    }
    if !skill_path_is_inside_store(&catalog.skills_dir, &entry.skill_path) {
        return Err("unknown skill".to_string());
    }
    Ok(entry)
}

pub fn activate_skill(
    config: &Config,
    skill_id: &str,
    selection_source: &str,
) -> Result<ResolvedSkillActivation, String> {
    let entry = resolve_skill_from_store(config, skill_id)?;
    let loaded = load_skill_instructions(&entry).map_err(|_| "skill unavailable".to_string())?;
    let prompt = parse_skill_prompt(&entry, &loaded).map_err(|status| match status {
        SkillPromptValidation::TooLarge => {
            "技能指令超过运行时容量限制；请精简完整指令段落后重试".to_string()
        }
        SkillPromptValidation::MissingInstructions => "技能缺少可执行指令".to_string(),
        SkillPromptValidation::Invalid => "技能提示结构无效".to_string(),
        SkillPromptValidation::ProviderIncompatible => {
            "技能与当前模型服务不兼容".to_string()
        }
        SkillPromptValidation::Valid => "技能提示校验失败".to_string(),
    })?;
    let provider_envelope = prompt.provider_envelope();
    let selection_source = normalize_selection_source(selection_source);
    let activation = ResolvedSkillActivation {
        skill_id: entry.id.clone(),
        display_name: entry.display_name.clone(),
        version: entry.version.clone(),
        selection_source: selection_source.clone(),
        instructions: prompt.instructions,
        resource_prompt: super::catalog::resource_index_prompt(
            &prompt.resource_index,
            entry.has_scripts,
        ),
        source: prompt.source,
        raw_skill_chars: prompt.raw_skill_chars,
        active_skill_chars: prompt.active_skill_chars,
        validation: prompt.validation,
        provider_envelope,
    };
    info!(
        skill_selected = true,
        skill_id = activation.skill_id.as_str(),
        selection_source = selection_source.as_str(),
        validation = ?activation.validation,
        raw_skill_chars = activation.raw_skill_chars,
        active_skill_chars = activation.active_skill_chars,
    );
    Ok(activation)
}

pub fn apply_skill_runtime_prompt(
    system_prompt: &mut Option<String>,
    config: &Config,
    invocations: &[SkillInvocation],
) -> SkillRuntimePromptResult {
    if !config.skills.open_skills_enabled {
        return SkillRuntimePromptResult::default();
    }
    let catalog = cached_skill_catalog(config);
    let catalog_section = catalog_prompt_section_with_limits(
        &catalog,
        config.skills.catalog_prompt_limit,
        config.skills.catalog_description_limit,
    );
    let mut activated = Vec::new();
    let mut errors = Vec::new();
    for invocation in invocations.iter().take(MAX_ACTIVE_SKILLS) {
        let source = normalize_selection_source(&invocation.source);
        match activate_skill(config, &invocation.skill_id, &source) {
            Ok(item) => activated.push(item),
            Err(error) => {
                errors.push(format!("{}: {error}", invocation.skill_id));
                tracing::info!(
                    skill_selected = false,
                    skill_id = invocation.skill_id.as_str(),
                    selection_source = source,
                    error = error.as_str()
                );
            }
        }
    }

    let mut addition = catalog_section;
    if let Some(active) = activated.first() {
        addition.push_str("\n\n");
        addition.push_str(&active.provider_envelope);
        addition.push_str(
            "\n\nThe explicitly selected skill is already active for this request. Do not call `use_skill` for it again; execute the user's request directly.",
        );
    } else {
        addition.push_str(
            "\nNo skill is fully loaded yet. Call `use_skill` when a catalog entry matches the task.\n",
        );
    }

    if addition.trim().is_empty() {
        return SkillRuntimePromptResult {
            catalog_count: catalog.entries.len(),
            activated,
            errors,
        };
    }
    let current = system_prompt.take().unwrap_or_default();
    *system_prompt = Some(format!("{current}\n{addition}"));
    SkillRuntimePromptResult {
        catalog_count: catalog
            .entries
            .iter()
            .filter(|entry| entry.runtime_visible)
            .count(),
        activated,
        errors,
    }
}

pub fn activation_system_message(tool_name: &str, tool_result: &str) -> Option<String> {
    if tool_name != "use_skill" {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(tool_result).ok()?;
    if parsed.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return None;
    }
    if parsed.get("already_active").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }
    let skill_id = parsed.get("skill_id")?.as_str()?;
    let name = parsed
        .get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or(skill_id);
    let envelope = parsed.get("provider_envelope")?.as_str()?;
    if envelope.trim().is_empty() {
        return None;
    }
    Some(format!(
        "{envelope}\n\nSkill `{skill_id}` ({name}) is now active for this request. Continue the user's request directly and do not call `use_skill` again."
    ))
}

const ACTIVE_SKILL_MARKER: &str = "## Active Skill";

pub fn is_skill_working_context_system(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with(ACTIVE_SKILL_MARKER) || trimmed.contains("\n## Active Skill")
}

pub fn strip_active_skill_section(content: &str) -> String {
    if let Some(index) = content.find("\n## Active Skill") {
        return content[..index].trim_end().to_string();
    }
    if content.trim_start().starts_with(ACTIVE_SKILL_MARKER) {
        return String::new();
    }
    content.to_string()
}

pub(crate) fn compact_use_skill_payload(content: &str) -> String {
    let Ok(mut outer) = serde_json::from_str::<serde_json::Value>(content) else {
        return content.to_string();
    };
    let Some(inner_raw) = outer.get("content").and_then(|value| value.as_str()) else {
        return content.to_string();
    };
    let Ok(mut inner) = serde_json::from_str::<serde_json::Value>(inner_raw) else {
        return content.to_string();
    };
    let looks_like_use_skill = inner.get("skill_id").is_some()
        && (inner.get("instructions").is_some() || inner.get("resource_prompt").is_some());
    if !looks_like_use_skill {
        return content.to_string();
    }
    if let Some(object) = inner.as_object_mut() {
        object.remove("instructions");
        object.remove("resource_prompt");
        object.remove("provider_envelope");
        object.insert(
            "instructions_loaded_into_active_context".into(),
            serde_json::Value::Bool(true),
        );
        object.insert("working_context_stripped".into(), serde_json::Value::Bool(true));
    }
    if let Some(object) = outer.as_object_mut() {
        object.insert("content".into(), serde_json::Value::String(inner.to_string()));
    }
    outer.to_string()
}

/// Drop request-scoped Skill instructions from persisted / next-turn history.
/// Catalog metadata in the bootstrap prompt is kept.
pub fn sanitize_skill_working_context(
    messages: Vec<crate::providers::ChatMessage>,
) -> Vec<crate::providers::ChatMessage> {
    messages
        .into_iter()
        .filter_map(|mut message| {
            if message.role == "system" {
                if message.content.trim_start().starts_with(ACTIVE_SKILL_MARKER) {
                    return None;
                }
                message.content = strip_active_skill_section(&message.content);
                if message.content.trim().is_empty() {
                    return None;
                }
                return Some(message);
            }
            if message.role == "tool" {
                message.content = compact_use_skill_payload(&message.content);
            }
            Some(message)
        })
        .collect()
}

pub fn skill_path_is_inside_store(store: &Path, skill_path: &Path) -> bool {
    let Ok(store) = store.canonicalize() else {
        return false;
    };
    let Ok(path) = skill_path.canonicalize() else {
        return false;
    };
    path.starts_with(&store)
}

pub fn normalize_invocation_id(skill_id: &str) -> String {
    normalize_skill_id(skill_id)
}

pub fn normalize_selection_source(raw: &str) -> String {
    match raw.trim() {
        "" | "slash_command" | "explicit" | "explicit_slash" => "explicit_slash".to_string(),
        "auto_use_skill" | "auto" => "auto_use_skill".to_string(),
        other => other.to_string(),
    }
}

pub fn invocations_from_inbound_metadata(
    metadata: &std::collections::HashMap<String, serde_json::Value>,
) -> Vec<SkillInvocation> {
    parse_skill_invocations(
        metadata
            .get("skill_invocations")
            .or_else(|| metadata.get("skillInvocations")),
    )
}
