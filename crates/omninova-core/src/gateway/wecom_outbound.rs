//! WeCom outbound helpers.
//!
//! Phase 1A.5: Real implementation via mpsc channel to writer loop.

use super::wecom_protocol::{build_respond_envelope, build_respond_envelope_with_options, WecomReplyContext};

/// Build reply JSON payload for a WeCom callback.
///
/// This function builds the envelope but actual sending is done via
/// the mpsc channel to the WebSocket writer loop.
pub fn build_reply(text: &str, req_id: &str) -> String {
    let env = build_respond_envelope(req_id, text);
    serde_json::to_string(&env).unwrap_or_default()
}

/// Build reply with extended options (for Phase 1B+).
pub fn build_reply_with_options(text: &str, req_id: &str, ctx: &WecomReplyContext) -> String {
    let env = build_respond_envelope_with_options(req_id, text, ctx);
    serde_json::to_string(&env).unwrap_or_default()
}
