use crate::config::Config;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 数字员工模板：按垂直角色（如 SRE / 运维专家）建立的专属对话模版。
/// 每个员工有独立人设(prompt)、精简绑定的技能、专属 MCP 配置（存储保留）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmployeeManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: i64,
    /// 关联的全局/工作区技能名称（用于在公共技能池中过滤）。
    #[serde(default)]
    pub skill_ids: Vec<String>,
    /// 该员工专属的 MCP 服务器配置（不透明 JSON，存储保留；当前运行时不执行）。
    #[serde(default)]
    pub mcp_servers: serde_json::Value,
    /// 所属类型/分类，空表示「其它」。
    #[serde(default)]
    pub r#type: String,
    /// 来源：local（自建）、remote（安装）。
    #[serde(default)]
    pub from: String,
}

fn default_true() -> bool {
    true
}

/// 列表展示用的精简结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmployeeSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub enabled: bool,
    pub created_at: i64,
    pub skill_ids: Vec<String>,
    /// 该员工专属技能目录下检测到的技能名（+ skill_ids）。
    pub skill_names: Vec<String>,
    /// MCP 配置的 key 列表（展示用）。
    pub mcp_server_keys: Vec<String>,
    pub r#type: String,
    pub from: String,
}

#[derive(Debug, Clone)]
pub struct EmployeeStore {
    root: PathBuf,
    skills_root: PathBuf,
}

impl EmployeeStore {
    pub fn open(config: &Config) -> Self {
        Self {
            root: employees_root(config),
            skills_root: employee_skills_dir(config),
        }
    }

    pub fn list_summaries(&self) -> Vec<EmployeeSummary> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.root) else {
            return out;
        };
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if let Some(manifest) = self.load(&id) {
                out.push(self.summarize(&manifest));
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        out
    }

    pub fn load(&self, id: &str) -> Option<EmployeeManifest> {
        let id = id.trim();
        if id.is_empty() {
            return None;
        }
        let path = self.root.join(id).join("manifest.json");
        let data = fs::read_to_string(path).ok()?;
        let mut manifest: EmployeeManifest = serde_json::from_str(&data).ok()?;
        if manifest.id.is_empty() {
            manifest.id = id.to_string();
        }
        Some(manifest)
    }

    /// 新增或更新数字员工。id 由 name 生成（稳定 slug）；保存时保留历史 created_at。
    pub fn save(&self, mut manifest: EmployeeManifest) -> Result<EmployeeManifest> {
        let mut id = manifest.id.trim().to_string();
        if id.is_empty() {
            id = slugify(&manifest.name);
        }
        if id.is_empty() {
            anyhow::bail!("数字员工名称不能为空");
        }
        manifest.id = id.clone();

        let dir = self.root.join(&id);
        fs::create_dir_all(&dir)
            .with_context(|| format!("创建员工目录失败: {}", dir.display()))?;

        let manifest_path = dir.join("manifest.json");
        if manifest.created_at == 0 {
            if let Some(existing) = self.load(&id) {
                if existing.created_at != 0 {
                    manifest.created_at = existing.created_at;
                }
            }
        }
        if manifest.created_at == 0 {
            manifest.created_at = now_millis();
        }
        if manifest.from.is_empty() {
            manifest.from = "local".to_string();
        }
        if manifest.mcp_servers.is_null() {
            manifest.mcp_servers = serde_json::json!({});
        }

        let data = serde_json::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, data)
            .with_context(|| format!("写入 manifest 失败: {}", manifest_path.display()))?;
        Ok(manifest)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<EmployeeManifest> {
        let mut manifest = self
            .load(id)
            .ok_or_else(|| anyhow::anyhow!("数字员工不存在: {id}"))?;
        manifest.enabled = enabled;
        self.save(manifest)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(false);
        }
        let dir = self.root.join(id);
        if !dir.exists() {
            return Ok(false);
        }
        fs::remove_dir_all(&dir)?;
        let skills_dir = self.skills_root.join(id);
        if skills_dir.exists() {
            let _ = fs::remove_dir_all(&skills_dir);
        }
        Ok(true)
    }

    /// 该员工专属技能目录（每个子目录含一个 SKILL.md）。
    pub fn skills_dir(&self, id: &str) -> PathBuf {
        self.skills_root.join(id.trim())
    }

    fn summarize(&self, manifest: &EmployeeManifest) -> EmployeeSummary {
        let mut skill_names: Vec<String> = manifest.skill_ids.clone();
        let skills_dir = self.skills_dir(&manifest.id);
        if let Ok(entries) = fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.starts_with('.') && !skill_names.contains(&name) {
                        skill_names.push(name);
                    }
                }
            }
        }
        skill_names.sort();
        skill_names.dedup();

        let mut mcp_server_keys: Vec<String> = manifest
            .mcp_servers
            .as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();
        mcp_server_keys.sort();

        let type_val = if manifest.r#type.trim().is_empty() {
            "其它".to_string()
        } else {
            manifest.r#type.clone()
        };

        EmployeeSummary {
            id: manifest.id.clone(),
            name: if manifest.name.trim().is_empty() {
                manifest.id.clone()
            } else {
                manifest.name.clone()
            },
            description: manifest.description.clone(),
            prompt: manifest.prompt.clone(),
            enabled: manifest.enabled,
            created_at: manifest.created_at,
            skill_ids: manifest.skill_ids.clone(),
            skill_names,
            mcp_server_keys,
            r#type: type_val,
            from: manifest.from.clone(),
        }
    }
}

pub fn employees_root(config: &Config) -> PathBuf {
    config.workspace_dir.join("employees")
}

pub fn employee_skills_dir(config: &Config) -> PathBuf {
    config.workspace_dir.join("employee_skills")
}

/// 由展示名生成稳定 id：保留中英文与数字，其余替换为连字符。
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.trim().chars() {
        if ch.is_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 组装某个数字员工会话要注入的技能提示：仅加载该员工专属技能目录。
pub fn employee_skill_prompt(store: &EmployeeStore, id: &str) -> String {
    use crate::skills::{format_skills_prompt, load_skills_from_dir};
    let dir = store.skills_dir(id);
    let skills = load_skills_from_dir(&dir).unwrap_or_default();
    format_skills_prompt(&skills)
}
