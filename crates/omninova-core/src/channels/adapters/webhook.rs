use crate::channels::{ChannelKind, InboundMessage};
use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebhookInboundPayload {
    pub text: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

pub fn inbound_from_webhook(payload: WebhookInboundPayload) -> InboundMessage {
    InboundMessage {
        channel: ChannelKind::Webhook,
        user_id: payload.user_id,
        session_id: payload.session_id,
        text: payload.text,
        metadata: payload.metadata,
    }
}
