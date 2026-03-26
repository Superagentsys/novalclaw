use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub mbti_type: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListResponse {
    pub agents: Vec<Agent>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub message: String,
    pub agent_id: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub uptime: u64,
}

pub struct Client {
    client: reqwest::Client,
    base_url: String,
}

impl Client {
    pub fn new(base_url: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn list_agents(&self) -> Result<AgentListResponse> {
        let url = format!("{}/api/agents", self.base_url);
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        let agents: Vec<Agent> = response.json().await
            .context("Failed to parse response")?;

        let total = agents.len();
        Ok(AgentListResponse { agents, total })
    }

    pub async fn get_agent(&self, id: &str) -> Result<Agent> {
        let url = format!("{}/api/agents/{}", self.base_url, id);
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        response.json().await
            .context("Failed to parse response")
    }

    pub async fn create_agent(&self, name: &str, mbti: Option<&str>) -> Result<Agent> {
        let url = format!("{}/api/agents", self.base_url);
        
        let mut body = HashMap::new();
        body.insert("name", name);
        if let Some(mbti) = mbti {
            body.insert("mbti_type", mbti);
        }

        let response = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        response.json().await
            .context("Failed to parse response")
    }

    pub async fn delete_agent(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/agents/{}", self.base_url, id);
        let response = self.client
            .delete(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        Ok(())
    }

    pub async fn chat(&self, agent_id: &str, message: &str) -> Result<ChatResponse> {
        let url = format!("{}/api/agents/{}/chat", self.base_url, agent_id);
        
        let body = HashMap::from([
            ("message", message),
        ]);

        let response = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        response.json().await
            .context("Failed to parse response")
    }

    pub async fn get_status(&self) -> Result<StatusResponse> {
        let url = format!("{}/health", self.base_url);
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        response.json().await
            .context("Failed to parse response")
    }

    pub async fn enable_agent(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/agents/{}/enable", self.base_url, id);
        let response = self.client
            .post(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        Ok(())
    }

    pub async fn disable_agent(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/agents/{}/disable", self.base_url, id);
        let response = self.client
            .post(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        Ok(())
    }

    pub async fn get_agent_stats(&self, id: &str) -> Result<AgentStats> {
        let url = format!("{}/api/agents/{}/stats", self.base_url, id);
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        response.json().await
            .context("Failed to parse response")
    }

    pub async fn get_all_agent_stats(&self) -> Result<Vec<AgentStats>> {
        let url = format!("{}/api/agents/stats", self.base_url);
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {} - {}", status, text);
        }

        response.json().await
            .context("Failed to parse response")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStats {
    pub agent_id: String,
    pub agent_name: String,
    pub message_count: u64,
    pub avg_response_time_ms: u64,
    pub session_count: u64,
    pub last_active: String,
}
