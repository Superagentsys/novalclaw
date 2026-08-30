use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpResponse {
    pub jsonrpc: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AcpError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// Common ACP methods
pub const METHOD_AGENT_SPAWN: &str = "agent.spawn";
pub const METHOD_AGENT_KILL: &str = "agent.kill";
pub const METHOD_AGENT_LIST: &str = "agent.list";
pub const METHOD_CHANNEL_SEND: &str = "channel.send";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpawnParams {
    pub name: String,
    pub prompt: Option<String>,
    pub env: Option<HashMap<String, String>>,
}
