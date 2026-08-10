//! DingTalk advanced-card OpenAPI adapter.
//!
//! This module owns only create/update operations. It never logs platform
//! identifiers, credentials, request bodies, or response bodies.

use crate::channels::InboundMessage;
use crate::config::schema::DingtalkTransportMode;
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const CREATE_AND_DELIVER_URL: &str =
    "https://api.dingtalk.com/v1.0/card/instances/createAndDeliver";
pub const UPDATE_CARD_URL: &str = "https://api.dingtalk.com/v1.0/card/instances";
const HTTP_TIMEOUT_SECS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DingtalkCardAvailability {
    UnsupportedTransport,
    MissingTemplate,
    StreamDisconnected,
    MissingContext,
    Available,
}

impl DingtalkCardAvailability {
    pub const fn log_value(self) -> &'static str {
        match self {
            Self::UnsupportedTransport => "unsupported_transport",
            Self::MissingTemplate => "missing_template",
            Self::StreamDisconnected => "stream_disconnected",
            Self::MissingContext => "missing_context",
            Self::Available => "available",
        }
    }
}

/// Decide whether an advanced card can be created. This is deliberately pure:
/// it performs no network request and never inspects or logs platform IDs.
///
/// Card availability requires Stream to be truly registered (not just WebSocket open).
pub fn determine_card_availability(
    transport_mode: DingtalkTransportMode,
    template_configured: bool,
    stream_registered: bool,
    context_complete: bool,
) -> DingtalkCardAvailability {
    if transport_mode != DingtalkTransportMode::Stream {
        return DingtalkCardAvailability::UnsupportedTransport;
    }
    if !template_configured {
        return DingtalkCardAvailability::MissingTemplate;
    }
    if !stream_registered {
        return DingtalkCardAvailability::StreamDisconnected;
    }
    if !context_complete {
        return DingtalkCardAvailability::MissingContext;
    }
    DingtalkCardAvailability::Available
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DingtalkCardTarget {
    Group {
        open_conversation_id: String,
        robot_code: String,
        user_id: Option<String>,
    },
    Direct {
        user_id: String,
        robot_code: String,
    },
}

impl DingtalkCardTarget {
    pub fn from_inbound(
        inbound: &InboundMessage,
        fallback_robot_code: Option<&str>,
    ) -> Result<Self, String> {
        let raw = inbound.metadata.get("raw_payload");
        let conversation_type = raw
            .and_then(|value| value.get("conversationType"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let robot_code = inbound
            .metadata
            .get("robotCode")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                fallback_robot_code
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .ok_or_else(|| "missing_robot_code".to_string())?
            .to_string();
        let user_id = inbound
            .metadata
            .get("senderStaffId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| inbound.user_id.clone());

        if conversation_type == "2" {
            let open_conversation_id = inbound
                .session_id
                .clone()
                .or_else(|| {
                    inbound
                        .metadata
                        .get("conversationId")
                        .and_then(serde_json::Value::as_str)
                        .map(ToString::to_string)
                })
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "missing_conversation_id".to_string())?;
            return Ok(Self::Group {
                open_conversation_id,
                robot_code,
                user_id,
            });
        }

        let user_id = user_id.ok_or_else(|| "missing_sender_staff_id".to_string())?;
        Ok(Self::Direct {
            user_id,
            robot_code,
        })
    }
}

pub fn build_menu_create_payload(
    card_template_id: &str,
    out_track_id: &str,
    target: &DingtalkCardTarget,
) -> serde_json::Value {
    let panel = crate::gateway::agent_menu::build_agent_menu_panel();
    let mut body = serde_json::json!({
        "cardTemplateId": card_template_id,
        "outTrackId": out_track_id,
        "callbackType": "STREAM",
        "userIdType": 1,
        "cardData": {
            "cardParamMap": {
                "title": panel.title,
                "status": "READY",
                "status_text": "Gateway 已连接",
                "result": "请选择需要执行的操作",
                "last_action": "-"
            }
        }
    });

    match target {
        DingtalkCardTarget::Group {
            open_conversation_id,
            robot_code,
            user_id,
        } => {
            body["openSpaceId"] =
                serde_json::json!(format!("dtv1.card//IM_GROUP.{open_conversation_id}"));
            body["imGroupOpenSpaceModel"] = serde_json::json!({ "supportForward": true });
            body["imGroupOpenDeliverModel"] = serde_json::json!({ "robotCode": robot_code });
            if let Some(user_id) = user_id {
                body["userId"] = serde_json::json!(user_id);
            }
        }
        DingtalkCardTarget::Direct {
            user_id,
            robot_code,
        } => {
            body["openSpaceId"] = serde_json::json!(format!("dtv1.card//IM_ROBOT.{user_id}"));
            body["userId"] = serde_json::json!(user_id);
            body["imRobotOpenSpaceModel"] = serde_json::json!({ "supportForward": true });
            body["imRobotOpenDeliverModel"] = serde_json::json!({
                "robotCode": robot_code,
                "spaceType": "IM_ROBOT"
            });
        }
    }
    body
}

pub fn build_card_update_payload(
    out_track_id: &str,
    status: &str,
    status_text: &str,
    result: &str,
    last_action: &str,
) -> serde_json::Value {
    serde_json::json!({
        "outTrackId": out_track_id,
        "userIdType": 1,
        "cardData": {
            "cardParamMap": {
                "status": status,
                "status_text": status_text,
                "result": result,
                "last_action": last_action
            }
        },
        "cardUpdateOptions": {
            "updateCardDataByKey": true
        }
    })
}

pub async fn create_and_deliver_menu_card(
    access_token: &str,
    card_template_id: &str,
    target: &DingtalkCardTarget,
) -> Result<String, String> {
    if card_template_id.trim().is_empty() {
        return Err("missing_card_template_id".to_string());
    }
    let out_track_id = format!("omninova-menu-{}", uuid::Uuid::new_v4());
    let payload = build_menu_create_payload(card_template_id, &out_track_id, target);
    println!(
        "[dingtalk-card] create_start template_configured=true target_kind={} out_track_hash={}",
        match target {
            DingtalkCardTarget::Group { .. } => "group",
            DingtalkCardTarget::Direct { .. } => "direct",
        },
        opaque_id(&out_track_id)
    );
    match send_card_request("POST", CREATE_AND_DELIVER_URL, access_token, &payload).await {
        Ok(()) => {
            println!(
                "[dingtalk-card] create_ok out_track_hash={}",
                opaque_id(&out_track_id)
            );
            Ok(out_track_id)
        }
        Err(error) => {
            println!(
                "[dingtalk-card] create_failed reason={}",
                safe_error_kind(&error)
            );
            Err(error)
        }
    }
}

pub async fn update_card(
    access_token: &str,
    out_track_id: &str,
    status: &str,
    status_text: &str,
    result: &str,
    last_action: &str,
) -> Result<(), String> {
    if out_track_id.trim().is_empty() {
        return Err("missing_out_track_id".to_string());
    }
    let payload = build_card_update_payload(out_track_id, status, status_text, result, last_action);
    println!(
        "[dingtalk-card] update_start state={} out_track_hash={}",
        safe_state(status),
        opaque_id(out_track_id)
    );
    send_card_request("PUT", UPDATE_CARD_URL, access_token, &payload).await?;
    println!(
        "[dingtalk-card] update_ok state={} out_track_hash={}",
        safe_state(status),
        opaque_id(out_track_id)
    );
    Ok(())
}

async fn send_card_request(
    method: &str,
    url: &str,
    access_token: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|_| "card_http_client_error".to_string())?;
    let request = match method {
        "PUT" => client.put(url),
        _ => client.post(url),
    };
    let response = request
        .header("Content-Type", "application/json")
        .header("x-acs-dingtalk-access-token", access_token)
        .json(payload)
        .send()
        .await
        .map_err(|_| "card_network_error".to_string())?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|_| "card_response_read_error".to_string())?;
    let parsed = serde_json::from_str::<serde_json::Value>(&body).ok();
    let code = parsed
        .as_ref()
        .and_then(|value| value.get("code").or_else(|| value.get("errcode")))
        .map(|value| match value {
            serde_json::Value::String(value) => value.clone(),
            value => value.to_string(),
        })
        .unwrap_or_else(|| {
            if (200..300).contains(&status) {
                "0"
            } else {
                "unknown"
            }
            .into()
        });
    let message_len = parsed
        .as_ref()
        .and_then(|value| value.get("message").or_else(|| value.get("msg")))
        .and_then(serde_json::Value::as_str)
        .map(str::len)
        .unwrap_or(0);
    let success = (200..300).contains(&status) && matches!(code.as_str(), "0" | "OK" | "ok");
    println!(
        "[dingtalk-card] response operation={} http_status={} platform_code={} message_len={} body_len={}",
        if method == "PUT" { "update" } else { "create" },
        status,
        safe_code(&code),
        message_len,
        body.len()
    );
    if success {
        Ok(())
    } else {
        Err(format!(
            "card_platform_error:status={status}:code={}:message_len={message_len}",
            safe_code(&code)
        ))
    }
}

pub fn opaque_id(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(&digest[..6])
}

fn safe_code(code: &str) -> String {
    code.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        .take(64)
        .collect()
}

fn safe_state(state: &str) -> &'static str {
    match state {
        "READY" => "READY",
        "RUNNING" => "RUNNING",
        "SUCCESS" => "SUCCESS",
        "FAILED" => "FAILED",
        _ => "UNKNOWN",
    }
}

fn safe_error_kind(error: &str) -> String {
    error
        .split([':', '='])
        .next()
        .unwrap_or("unknown")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        .take(64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_availability_prioritizes_transport_and_stream_requirements() {
        use DingtalkCardAvailability::*;
        use DingtalkTransportMode::{Http, Stream};

        assert_eq!(
            determine_card_availability(Http, true, true, true),
            UnsupportedTransport
        );
        assert_eq!(
            determine_card_availability(Http, false, false, false),
            UnsupportedTransport
        );
        assert_eq!(
            determine_card_availability(Stream, false, true, true),
            MissingTemplate
        );
        assert_eq!(
            determine_card_availability(Stream, true, false, true),
            StreamDisconnected
        );
        assert_eq!(
            determine_card_availability(Stream, true, true, false),
            MissingContext
        );
        assert_eq!(
            determine_card_availability(Stream, true, true, true),
            Available
        );
    }

    #[test]
    fn group_payload_uses_stream_and_group_space() {
        let target = DingtalkCardTarget::Group {
            open_conversation_id: "conversation-secret".into(),
            robot_code: "robot-secret".into(),
            user_id: Some("user-secret".into()),
        };
        let payload = build_menu_create_payload("template", "track", &target);
        assert_eq!(payload["callbackType"], "STREAM");
        assert_eq!(payload["cardData"]["cardParamMap"]["status"], "READY");
        assert_eq!(
            payload["cardData"]["cardParamMap"]["status_text"],
            "Gateway 已连接"
        );
        assert_eq!(
            payload["cardData"]["cardParamMap"]["result"],
            "请选择需要执行的操作"
        );
        assert_eq!(payload["cardData"]["cardParamMap"]["last_action"], "-");
        assert!(payload["openSpaceId"]
            .as_str()
            .is_some_and(|value| value.starts_with("dtv1.card//IM_GROUP.")));
        assert_eq!(
            payload["imGroupOpenDeliverModel"]["robotCode"],
            "robot-secret"
        );
    }

    #[test]
    fn direct_payload_uses_robot_space() {
        let target = DingtalkCardTarget::Direct {
            user_id: "user-secret".into(),
            robot_code: "robot-secret".into(),
        };
        let payload = build_menu_create_payload("template", "track", &target);
        assert!(payload["openSpaceId"]
            .as_str()
            .is_some_and(|value| value.starts_with("dtv1.card//IM_ROBOT.")));
        assert_eq!(payload["imRobotOpenDeliverModel"]["spaceType"], "IM_ROBOT");
    }

    #[test]
    fn update_payload_is_incremental_and_uses_same_track_id() {
        let payload = build_card_update_payload(
            "same-track",
            "SUCCESS",
            "done",
            "status body",
            "gateway_status",
        );
        assert_eq!(payload["outTrackId"], "same-track");
        assert_eq!(payload["cardUpdateOptions"]["updateCardDataByKey"], true);
        assert_eq!(payload["cardData"]["cardParamMap"]["status"], "SUCCESS");
    }

    #[test]
    fn opaque_id_never_contains_original_identifier() {
        let hash = opaque_id("secret-conversation-identifier");
        assert_eq!(hash.len(), 12);
        assert!(!hash.contains("secret"));
    }
}
