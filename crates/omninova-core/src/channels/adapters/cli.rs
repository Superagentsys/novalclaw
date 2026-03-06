use crate::channels::{ChannelKind, InboundMessage};
use std::collections::HashMap;

pub fn inbound_from_cli(
    text: impl Into<String>,
    session_id: Option<String>,
    user_id: Option<String>,
) -> InboundMessage {
    InboundMessage {
        channel: ChannelKind::Cli,
        user_id,
        session_id,
        text: text.into(),
        metadata: HashMap::new(),
    }
}
