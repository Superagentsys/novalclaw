use crate::config::Config;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EstopState {
    #[serde(default)]
    pub paused: bool,
    pub level: Option<String>,
    pub domain: Option<String>,
    pub tool: Option<String>,
    pub reason: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EstopController {
    enabled: bool,
    state_file: PathBuf,
}

impl EstopController {
    pub fn from_config(config: &Config) -> Self {
        let state_file = config
            .security
            .estop
            .state_file
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| config.workspace_dir.join(".omninova-estop.json"));
        Self {
            enabled: config.security.estop.enabled,
            state_file,
        }
    }

    pub async fn load(&self) -> Result<EstopState> {
        if !self.enabled {
            return Ok(EstopState::default());
        }
        if !self.state_file.exists() {
            return Ok(EstopState::default());
        }
        let raw = tokio::fs::read_to_string(&self.state_file).await.unwrap_or_default();
        let parsed = serde_json::from_str::<EstopState>(&raw).unwrap_or_default();
        Ok(parsed)
    }

    pub async fn is_paused(&self) -> Result<bool> {
        Ok(self.load().await?.paused)
    }

    pub async fn pause(
        &self,
        level: Option<String>,
        domain: Option<String>,
        tool: Option<String>,
        reason: Option<String>,
    ) -> Result<EstopState> {
        if !self.enabled {
            return Ok(EstopState::default());
        }
        let state = EstopState {
            paused: true,
            level,
            domain,
            tool,
            reason,
            updated_at: Some(now_ts()),
        };
        self.save(&state).await?;
        Ok(state)
    }

    pub async fn resume(&self) -> Result<EstopState> {
        if !self.enabled {
            return Ok(EstopState::default());
        }
        let state = EstopState {
            paused: false,
            level: None,
            domain: None,
            tool: None,
            reason: None,
            updated_at: Some(now_ts()),
        };
        self.save(&state).await?;
        Ok(state)
    }

    async fn save(&self, state: &EstopState) -> Result<()> {
        if let Some(parent) = self.state_file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let raw = serde_json::to_string_pretty(state)?;
        tokio::fs::write(&self.state_file, raw).await?;
        Ok(())
    }
}

fn now_ts() -> String {
    time::OffsetDateTime::now_utc().unix_timestamp().to_string()
}
