use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
    Consumed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: String,
    /// Raw tool arguments. Used for exact matching and never displayed directly
    /// in approval UI when a separate safe_arguments value is available.
    pub arguments: serde_json::Value,
    /// Sanitized/truncated arguments for user-facing approval cards.
    #[serde(default = "default_safe_arguments")]
    pub safe_arguments: serde_json::Value,
    pub args_hash: String,
    pub action: Option<String>,
    pub risk_level: Option<String>,
    pub summary: Option<String>,
    pub reason: String,
    pub status: ApprovalStatus,
    pub created_at: String,
    pub updated_at: String,
    pub approved_by: Option<String>,
    pub reject_reason: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub resume_on_approve: bool,
}

fn default_safe_arguments() -> serde_json::Value {
    serde_json::Value::Null
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ApprovalStore {
    #[serde(default)]
    items: Vec<PendingApproval>,
    #[serde(skip)]
    loaded: bool,
}

type SharedApprovalStore = Arc<AsyncMutex<ApprovalStore>>;

fn shared_store(path: &Path) -> SharedApprovalStore {
    static MAP: OnceLock<StdMutex<HashMap<String, SharedApprovalStore>>> = OnceLock::new();
    let mut map = MAP
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .expect("approval store map lock poisoned");
    let key = path.to_string_lossy().to_string();
    map.entry(key)
        .or_insert_with(|| Arc::new(AsyncMutex::new(ApprovalStore::default())))
        .clone()
}

#[derive(Debug, Clone)]
pub struct ApprovalController {
    store_file: PathBuf,
    store: SharedApprovalStore,
}

impl ApprovalController {
    pub fn from_workspace(workspace_dir: &PathBuf) -> Self {
        let store_file = workspace_dir.join(".omninova-approvals.json");
        let store = shared_store(&store_file);
        Self { store_file, store }
    }

    pub async fn list(&self, pending_only: bool) -> Result<Vec<PendingApproval>> {
        let mut store = self.store.lock().await;
        self.ensure_loaded(&mut store).await?;
        Ok(if pending_only {
            store
                .items
                .iter()
                .filter(|item| item.status == ApprovalStatus::Pending)
                .cloned()
                .collect()
        } else {
            store.items.clone()
        })
    }

    pub async fn get(&self, id: &str) -> Result<Option<PendingApproval>> {
        let mut store = self.store.lock().await;
        self.ensure_loaded(&mut store).await?;
        Ok(store.items.iter().find(|item| item.id == id).cloned())
    }

    pub async fn create(
        &self,
        run_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        safe_arguments: serde_json::Value,
        action: &str,
        risk_level: &str,
        summary: &str,
        reason: &str,
    ) -> Result<PendingApproval> {
        let mut store = self.store.lock().await;
        self.ensure_loaded(&mut store).await?;
        let now = now_ts();
        let args_hash = hash_tool_args(tool_name, &arguments);
        let item = PendingApproval {
            id: format!("appr-{}", Uuid::new_v4()),
            run_id: Some(run_id.to_string()),
            tool_call_id: Some(tool_call_id.to_string()),
            tool_name: tool_name.to_string(),
            arguments: arguments.clone(),
            safe_arguments: if safe_arguments.is_null() {
                arguments.clone()
            } else {
                safe_arguments
            },
            args_hash,
            action: if action.is_empty() {
                None
            } else {
                Some(action.to_string())
            },
            risk_level: if risk_level.is_empty() {
                None
            } else {
                Some(risk_level.to_string())
            },
            summary: if summary.is_empty() {
                None
            } else {
                Some(summary.to_string())
            },
            reason: reason.to_string(),
            status: ApprovalStatus::Pending,
            created_at: now.clone(),
            updated_at: now,
            approved_by: None,
            reject_reason: None,
            session_id: None,
            task_id: None,
            channel: None,
            resume_on_approve: false,
        };
        store.items.push(item.clone());
        self.save_locked(&store).await?;
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

    /// Marks every Pending approval for a run as Cancelled. Returns the number
    /// of approvals that transitioned from Pending to Cancelled.
    pub async fn cancel_for_run(&self, run_id: &str) -> Result<usize> {
        let mut store = self.store.lock().await;
        self.ensure_loaded(&mut store).await?;
        let now = now_ts();
        let mut changed = 0usize;
        for item in store.items.iter_mut() {
            if item.run_id.as_deref() == Some(run_id) && item.status == ApprovalStatus::Pending {
                item.status = ApprovalStatus::Cancelled;
                item.updated_at = now.clone();
                changed += 1;
            }
        }
        if changed > 0 {
            self.save_locked(&store).await?;
        }
        Ok(changed)
    }

    pub async fn attach_resume_context(
        &self,
        id: &str,
        session_id: Option<String>,
        task_id: Option<String>,
        channel: Option<String>,
    ) -> Result<()> {
        let mut store = self.store.lock().await;
        self.ensure_loaded(&mut store).await?;
        if let Some(item) = store.items.iter_mut().find(|item| item.id == id) {
            item.session_id = session_id;
            item.task_id = task_id;
            item.channel = channel;
            item.resume_on_approve = true;
            item.updated_at = now_ts();
        }
        self.save_locked(&store).await?;
        Ok(())
    }

    /// If an approved request exists for the exact run+tool_call identity, mark
    /// it Consumed and return it. This is the exactly-once authorization gate.
    pub async fn consume_matching_grant(
        &self,
        run_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<Option<PendingApproval>> {
        let hash = hash_tool_args(tool_name, arguments);
        let mut store = self.store.lock().await;
        self.ensure_loaded(&mut store).await?;
        let idx = store.items.iter().position(|item| {
            item.status == ApprovalStatus::Approved
                && item.run_id.as_deref() == Some(run_id)
                && item.tool_call_id.as_deref() == Some(tool_call_id)
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
        self.save_locked(&store).await?;
        Ok(Some(item))
    }

    async fn update_status(
        &self,
        id: &str,
        status: ApprovalStatus,
        approved_by: Option<String>,
        reject_reason: Option<String>,
    ) -> Result<PendingApproval> {
        let mut store = self.store.lock().await;
        self.ensure_loaded(&mut store).await?;
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
        self.save_locked(&store).await?;
        Ok(out)
    }

    async fn ensure_loaded(&self, store: &mut ApprovalStore) -> Result<()> {
        if store.loaded {
            return Ok(());
        }
        if self.store_file.exists() {
            let raw = tokio::fs::read_to_string(&self.store_file).await?;
            if let Ok(mut file_store) = serde_json::from_str::<ApprovalStore>(&raw) {
                let mut had_pending = false;
                let now = now_ts();
                for item in file_store.items.iter_mut() {
                    if item.status == ApprovalStatus::Pending {
                        item.status = ApprovalStatus::Cancelled;
                        item.updated_at = now.clone();
                        had_pending = true;
                    }
                }
                if had_pending {
                    // Old pending approvals from a previous process must never
                    // become executable after restart.
                    *store = file_store;
                    store.loaded = true;
                    self.save_locked(store).await?;
                    return Ok(());
                }
                *store = file_store;
            }
        }
        store.loaded = true;
        Ok(())
    }

    async fn save_locked(&self, store: &ApprovalStore) -> Result<()> {
        if let Some(parent) = self.store_file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let raw = serde_json::to_string_pretty(store)?;
        let tmp = self
            .store_file
            .with_extension(format!("tmp-{}", Uuid::new_v4()));
        tokio::fs::write(&tmp, raw).await?;
        tokio::fs::rename(&tmp, &self.store_file).await?;
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

    fn temp_controller(label: &str) -> ApprovalController {
        let dir = std::env::temp_dir().join(format!("omninova-approval-{label}-{}", Uuid::new_v4()));
        let controller = ApprovalController::from_workspace(&dir);
        clean_on_drop(dir);
        controller
    }

    fn clean_on_drop(_dir: PathBuf) {}

    #[test]
    fn hash_is_stable() {
        let args = serde_json::json!({"command": "ls"});
        let a = hash_tool_args("shell", &args);
        let b = hash_tool_args("shell", &args);
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn read_only_allow_is_not_created_by_controller() {
        // Controller-level test only checks identity/consume semantics; policy
        // Allow/Ask is decided in SecurityContext.
        let controller = temp_controller("readonly");
        let pending = controller
            .create(
                "run-1",
                "call-1",
                "file_read",
                serde_json::json!({"path": "README.md"}),
                serde_json::json!({"path": "README.md"}),
                "读取文件",
                "low",
                "读取文件",
                "read-only",
            )
            .await
            .unwrap();
        assert_eq!(pending.status, ApprovalStatus::Pending);
    }

    #[tokio::test]
    async fn create_binds_run_and_tool_call() {
        let controller = temp_controller("identity");
        let pending = controller
            .create(
                "run-a",
                "call-a",
                "write_file",
                serde_json::json!({"path": "/tmp/a"}),
                serde_json::json!({"path": "/tmp/a"}),
                "写入文件",
                "medium",
                "写入文件",
                "side effect",
            )
            .await
            .unwrap();
        assert_eq!(pending.run_id.as_deref(), Some("run-a"));
        assert_eq!(pending.tool_call_id.as_deref(), Some("call-a"));
    }

    #[tokio::test]
    async fn approve_consumes_exactly_once_for_exact_identity() {
        let controller = temp_controller("once");
        let pending = controller
            .create(
                "run-a",
                "call-a",
                "write_file",
                serde_json::json!({"path": "/tmp/a"}),
                serde_json::json!({"path": "/tmp/a"}),
                "写入文件",
                "medium",
                "写入文件",
                "side effect",
            )
            .await
            .unwrap();

        controller.approve(&pending.id, Some("user".to_string())).await.unwrap();
        let first = controller
            .consume_matching_grant("run-a", "call-a", "write_file", &serde_json::json!({"path": "/tmp/a"}))
            .await
            .unwrap();
        assert!(first.is_some());
        let second = controller
            .consume_matching_grant("run-a", "call-a", "write_file", &serde_json::json!({"path": "/tmp/a"}))
            .await
            .unwrap();
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn duplicate_approve_is_blocked() {
        let controller = temp_controller("dup");
        let pending = controller
            .create(
                "run-a",
                "call-a",
                "shell",
                serde_json::json!({"command": "ls"}),
                serde_json::json!({"command": "ls"}),
                "执行命令",
                "medium",
                "执行命令",
                "shell",
            )
            .await
            .unwrap();
        controller.approve(&pending.id, None).await.unwrap();
        let err = controller.approve(&pending.id, None).await;
        assert!(err.is_err(), "second approve must fail");
    }

    #[tokio::test]
    async fn reject_prevents_consumption() {
        let controller = temp_controller("reject");
        let pending = controller
            .create(
                "run-a",
                "call-a",
                "write_file",
                serde_json::json!({"path": "/tmp/a"}),
                serde_json::json!({"path": "/tmp/a"}),
                "写入文件",
                "medium",
                "写入文件",
                "side effect",
            )
            .await
            .unwrap();
        controller.reject(&pending.id, Some("user_rejected".to_string())).await.unwrap();
        let consumed = controller
            .consume_matching_grant("run-a", "call-a", "write_file", &serde_json::json!({"path": "/tmp/a"}))
            .await
            .unwrap();
        assert!(consumed.is_none());
    }

    #[tokio::test]
    async fn cancel_for_run_cancels_pending_and_blocks_later_approve() {
        let controller = temp_controller("cancel");
        let pending = controller
            .create(
                "run-a",
                "call-a",
                "write_file",
                serde_json::json!({"path": "/tmp/a"}),
                serde_json::json!({"path": "/tmp/a"}),
                "写入文件",
                "medium",
                "写入文件",
                "side effect",
            )
            .await
            .unwrap();
        let changed = controller.cancel_for_run("run-a").await.unwrap();
        assert_eq!(changed, 1);
        let stored = controller.get(&pending.id).await.unwrap().unwrap();
        assert_eq!(stored.status, ApprovalStatus::Cancelled);
        let err = controller.approve(&pending.id, None).await;
        assert!(err.is_err(), "approve after cancel must fail");
    }

    #[tokio::test]
    async fn cross_run_consumption_is_rejected() {
        let controller = temp_controller("cross");
        let pending = controller
            .create(
                "run-a",
                "call-a",
                "write_file",
                serde_json::json!({"path": "/tmp/a"}),
                serde_json::json!({"path": "/tmp/a"}),
                "写入文件",
                "medium",
                "写入文件",
                "side effect",
            )
            .await
            .unwrap();
        controller.approve(&pending.id, None).await.unwrap();
        let consumed = controller
            .consume_matching_grant("run-b", "call-a", "write_file", &serde_json::json!({"path": "/tmp/a"}))
            .await
            .unwrap();
        assert!(consumed.is_none(), "run mismatch must not consume");
    }

    #[tokio::test]
    async fn tool_call_id_mismatch_is_rejected() {
        let controller = temp_controller("call-mismatch");
        let pending = controller
            .create(
                "run-a",
                "call-a",
                "write_file",
                serde_json::json!({"path": "/tmp/a"}),
                serde_json::json!({"path": "/tmp/a"}),
                "写入文件",
                "medium",
                "写入文件",
                "side effect",
            )
            .await
            .unwrap();
        controller.approve(&pending.id, None).await.unwrap();
        let consumed = controller
            .consume_matching_grant("run-a", "call-b", "write_file", &serde_json::json!({"path": "/tmp/a"}))
            .await
            .unwrap();
        assert!(consumed.is_none(), "tool_call mismatch must not consume");
    }

    #[tokio::test]
    async fn approve_reject_race_first_wins() {
        let controller = temp_controller("race");
        let pending = controller
            .create(
                "run-a",
                "call-a",
                "shell",
                serde_json::json!({"command": "ls"}),
                serde_json::json!({"command": "ls"}),
                "执行命令",
                "medium",
                "执行命令",
                "shell",
            )
            .await
            .unwrap();

        let approve = controller.approve(&pending.id, None);
        let reject = controller.reject(&pending.id, Some("nope".to_string()));
        let (a, r) = tokio::join!(approve, reject);
        assert!(
            a.is_ok() != r.is_ok(),
            "exactly one terminal decision must win"
        );
    }
}