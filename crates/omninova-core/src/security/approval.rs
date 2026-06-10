use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Consumed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub args_hash: String,
    pub reason: String,
    pub status: ApprovalStatus,
    pub created_at: String,
    pub updated_at: String,
    pub approved_by: Option<String>,
    pub reject_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ApprovalStore {
    #[serde(default)]
    items: Vec<PendingApproval>,
}

#[derive(Debug, Clone)]
pub struct ApprovalController {
    store_file: PathBuf,
}

impl ApprovalController {
    pub fn from_workspace(workspace_dir: &PathBuf) -> Self {
        Self {
            store_file: workspace_dir.join(".omninova-approvals.json"),
        }
    }

    pub async fn list(&self, pending_only: bool) -> Result<Vec<PendingApproval>> {
        let store = self.load().await?;
        Ok(if pending_only {
            store
                .items
                .into_iter()
                .filter(|item| item.status == ApprovalStatus::Pending)
                .collect()
        } else {
            store.items
        })
    }

    pub async fn create(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        reason: &str,
    ) -> Result<PendingApproval> {
        let mut store = self.load().await?;
        let now = now_ts();
        let args_hash = hash_tool_args(tool_name, &arguments);
        let item = PendingApproval {
            id: format!("appr-{}", Uuid::new_v4()),
            tool_name: tool_name.to_string(),
            arguments,
            args_hash,
            reason: reason.to_string(),
            status: ApprovalStatus::Pending,
            created_at: now.clone(),
            updated_at: now,
            approved_by: None,
            reject_reason: None,
        };
        store.items.push(item.clone());
        self.save(&store).await?;
        Ok(item)
    }

    pub async fn approve(&self, id: &str, approved_by: Option<String>) -> Result<PendingApproval> {
        self.update_status(id, ApprovalStatus::Approved, approved_by, None)
            .await
    }

    pub async fn reject(
        &self,
        id: &str,
        reject_reason: Option<String>,
    ) -> Result<PendingApproval> {
        self.update_status(id, ApprovalStatus::Rejected, None, reject_reason)
            .await
    }

    /// If a matching approved (not yet consumed) request exists, mark consumed and return true.
    pub async fn consume_matching_grant(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<Option<PendingApproval>> {
        let hash = hash_tool_args(tool_name, arguments);
        let mut store = self.load().await?;
        let idx = store.items.iter().position(|item| {
            item.status == ApprovalStatus::Approved
                && item.tool_name == tool_name
                && item.args_hash == hash
        });
        let Some(idx) = idx else {
            return Ok(None);
        };
        let mut item = store.items[idx].clone();
        item.status = ApprovalStatus::Consumed;
        item.updated_at = now_ts();
        store.items[idx] = item.clone();
        self.save(&store).await?;
        Ok(Some(item))
    }

    async fn update_status(
        &self,
        id: &str,
        status: ApprovalStatus,
        approved_by: Option<String>,
        reject_reason: Option<String>,
    ) -> Result<PendingApproval> {
        let mut store = self.load().await?;
        let item = store
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| anyhow::anyhow!("approval request not found: {id}"))?;
        if item.status != ApprovalStatus::Pending {
            anyhow::bail!("approval request {id} is not pending");
        }
        item.status = status;
        item.updated_at = now_ts();
        item.approved_by = approved_by;
        item.reject_reason = reject_reason;
        let out = item.clone();
        self.save(&store).await?;
        Ok(out)
    }

    async fn load(&self) -> Result<ApprovalStore> {
        if !self.store_file.exists() {
            return Ok(ApprovalStore::default());
        }
        let raw = tokio::fs::read_to_string(&self.store_file).await?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    async fn save(&self, store: &ApprovalStore) -> Result<()> {
        if let Some(parent) = self.store_file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let raw = serde_json::to_string_pretty(store)?;
        tokio::fs::write(&self.store_file, raw).await?;
        Ok(())
    }
}

pub fn hash_tool_args(tool_name: &str, arguments: &serde_json::Value) -> String {
    let payload = serde_json::json!({
        "tool": tool_name,
        "arguments": arguments,
    });
    let mut hasher = Sha256::new();
    hasher.update(payload.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn now_ts() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable() {
        let args = serde_json::json!({"command": "ls"});
        let a = hash_tool_args("shell", &args);
        let b = hash_tool_args("shell", &args);
        assert_eq!(a, b);
    }
}
