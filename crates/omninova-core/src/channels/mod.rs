pub mod adapters;

use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Cli,
    Web,
    Telegram,
    Discord,
    Slack,
    Whatsapp,
    Matrix,
    Wechat,
    Feishu,
    Lark,
    Dingtalk,
    Email,
    Webhook,
    Other(String),
}

impl Default for ChannelKind {
    fn default() -> Self {
        Self::Cli
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct InboundMessage {
    #[serde(default)]
    pub channel: ChannelKind,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct OutboundMessage {
    #[serde(default)]
    pub channel: ChannelKind,
    pub session_id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}
