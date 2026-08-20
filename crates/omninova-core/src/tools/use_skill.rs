use crate::config::Config;
use crate::skills::activation::activate_skill;
use crate::skills::catalog::MAX_ACTIVE_SKILLS;
use crate::tools::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};

/// Limits auto `use_skill` to a single active skill per request.
#[derive(Debug, Default)]
pub struct SkillActivationGate {
    explicit_id: Option<String>,
    active_id: Mutex<Option<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillActivationOutcome {
    NewlyActivated,
    AlreadyActive,
}

impl SkillActivationGate {
    pub fn with_explicit(skill_id: Option<String>) -> Self {
        Self {
            explicit_id: skill_id.clone(),
            active_id: Mutex::new(skill_id),
        }
    }

    fn try_activate(&self, skill_id: &str) -> Result<SkillActivationOutcome, String> {
        if let Some(explicit) = &self.explicit_id {
            if explicit != skill_id {
                return Err("a skill is already explicitly selected for this request".to_string());
            }
        }
        let mut active = self
            .active_id
            .lock()
            .map_err(|_| "skill activation lock".to_string())?;
        if let Some(current) = active.as_ref() {
            if current == skill_id {
                return Ok(SkillActivationOutcome::AlreadyActive);
            }
            if MAX_ACTIVE_SKILLS <= 1 {
                return Err("only one skill may be active for this request".to_string());
            }
        }
        *active = Some(skill_id.to_string());
        Ok(SkillActivationOutcome::NewlyActivated)
    }
}

pub struct UseSkillTool {
    config: Config,
    gate: Arc<SkillActivationGate>,
}

impl UseSkillTool {
    pub fn new(config: Config, gate: Arc<SkillActivationGate>) -> Self {
        Self { config, gate }
    }
}

#[async_trait]
impl Tool for UseSkillTool {
    fn name(&self) -> &str {
        "use_skill"
    }

    fn description(&self) -> &str {
        "Load the full instructions for one installed skill from the catalog. Pass the catalog `skill_id` (for example skill:baichen-legal). Do not execute skill scripts."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "skill_id": {
                    "type": "string",
                    "description": "Catalog skill id such as skill:baichen-legal or the skill slug"
                }
            },
            "required": ["skill_id"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let skill_id = args
            .get("skill_id")
            .or_else(|| args.get("skillId"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if skill_id.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("skill_id is required".to_string()),
            });
        }
        match activate_skill(&self.config, &skill_id, "auto_use_skill") {
            Ok(activation) => {
                match self.gate.try_activate(&activation.skill_id) {
                    Err(error) => {
                        return Ok(ToolResult {
                            success: false,
                            output: json!({
                                "ok": false,
                                "error": "skill unavailable",
                                "detail": error,
                            })
                            .to_string(),
                            error: Some("skill unavailable".to_string()),
                        });
                    }
                    Ok(SkillActivationOutcome::AlreadyActive) => {
                        return Ok(ToolResult {
                            success: true,
                            output: json!({
                                "ok": true,
                                "already_active": true,
                                "skill_id": activation.skill_id,
                                "display_name": activation.display_name,
                                "version": activation.version,
                                "selection_source": activation.selection_source,
                            })
                            .to_string(),
                            error: None,
                        });
                    }
                    Ok(SkillActivationOutcome::NewlyActivated) => {}
                }
                Ok(ToolResult {
                    success: true,
                    output: json!({
                        "ok": true,
                        "skill_id": activation.skill_id,
                        "display_name": activation.display_name,
                        "version": activation.version,
                        "selection_source": activation.selection_source,
                        "instructions": activation.instructions,
                        "resource_prompt": activation.resource_prompt,
                        "provider_envelope": activation.provider_envelope,
                        "source": activation.source,
                        "raw_skill_chars": activation.raw_skill_chars,
                        "active_skill_chars": activation.active_skill_chars,
                        "validation": activation.validation,
                    })
                    .to_string(),
                    error: None,
                })
            }
            Err(error) => Ok(ToolResult {
                success: false,
                output: json!({
                    "ok": false,
                    "error": "skill unavailable",
                    "detail": error,
                })
                .to_string(),
                error: Some("skill unavailable".to_string()),
            }),
        }
    }
}
