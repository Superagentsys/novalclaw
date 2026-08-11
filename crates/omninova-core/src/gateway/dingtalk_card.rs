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

/// Conversation kind carried by the **robot callback** topic
/// (`/v1.0/im/bot/messages/get`): `"1"` = single chat, `"2"` = group chat.
///
/// This deliberately does NOT reuse the interactive-card API conversation
/// type codes (where the numbering differs). It is named explicitly
/// `DingtalkRobotConversationType` so the two APIs can never be mixed up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DingtalkRobotConversationType {
    Direct,
    Group,
}

impl DingtalkRobotConversationType {
    /// Parse a robot callback conversationType. Accepts both `"1"` / `"2"`
    /// strings and numeric `1` / `2` scalars (delivery-path drift).
    /// Unknown or missing values are an error: a mistargeted card is worse
    /// than a text fallback.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim() {
            "1" => Ok(Self::Direct),
            "2" => Ok(Self::Group),
            _ => Err("unknown_robot_conversation_type".to_string()),
        }
    }

    pub const fn log_value(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Group => "group",
        }
    }
}

/// Normalize a JSON scalar (string or number) to its string form.
fn json_scalar_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

impl DingtalkCardTarget {
    pub fn from_inbound(
        inbound: &InboundMessage,
        fallback_robot_code: Option<&str>,
    ) -> Result<Self, String> {
        // Robot callback conversationType may arrive at metadata top level
        // (Stream path) or nested inside raw_payload (HTTP webhook path).
        // Both use robot callback semantics: "1"=single, "2"=group.
        let conversation_type = inbound
            .metadata
            .get("conversationType")
            .and_then(json_scalar_to_string)
            .or_else(|| {
                inbound
                    .metadata
                    .get("raw_payload")
                    .and_then(|raw| raw.get("conversationType"))
                    .and_then(json_scalar_to_string)
            })
            .unwrap_or_default();
        let conversation_kind = DingtalkRobotConversationType::parse(&conversation_type)?;
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

        println!(
            "[dingtalk-routing] source=robot_callback conversation_kind={} conversation_id_present={} sender_staff_id_present={} robot_code_present={}",
            conversation_kind.log_value(),
            inbound
                .session_id
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
                || inbound
                    .metadata
                    .get("conversationId")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty()),
            user_id.as_ref().is_some(),
            inbound.metadata.contains_key("robotCode")
        );

        match conversation_kind {
            DingtalkRobotConversationType::Group => {
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
                Ok(Self::Group {
                    open_conversation_id,
                    robot_code,
                    user_id,
                })
            }
            DingtalkRobotConversationType::Direct => {
                let user_id = user_id.ok_or_else(|| "missing_sender_staff_id".to_string())?;
                Ok(Self::Direct {
                    user_id,
                    robot_code,
                })
            }
        }
    }
}

pub fn build_menu_create_payload(
    card_template_id: &str,
    out_track_id: &str,
    target: &DingtalkCardTarget,
) -> serde_json::Value {
    let panel = crate::gateway::agent_menu::build_agent_menu_panel();
    let button_groups = build_card_button_groups();
    let mut body = serde_json::json!({
        "cardTemplateId": card_template_id,
        "outTrackId": out_track_id,
        "callbackType": "STREAM",
        "userIdType": 1,
        "cardData": {
            "cardParamMap": {
                "title": panel.title,
                "status": card_status_label("READY"),
                "status_text": "Gateway 已连接",
                "result": "请选择需要执行的操作",
                "last_action": "-",
                "btns": button_groups.btns,
                "primary_actions": button_groups.primary_actions,
                "monitor_actions": button_groups.monitor_actions,
                "help_actions": button_groups.help_actions
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
    let panel = crate::gateway::agent_menu::build_agent_menu_panel();
    let button_groups = build_card_button_groups();
    serde_json::json!({
        "outTrackId": out_track_id,
        "userIdType": 1,
        "cardData": {
            "cardParamMap": {
                "title": panel.title,
                "status": card_status_label(status),
                "status_text": status_text,
                "result": result,
                "last_action": last_action,
                "btns": button_groups.btns,
                "primary_actions": button_groups.primary_actions,
                "monitor_actions": button_groups.monitor_actions,
                "help_actions": button_groups.help_actions
            }
        },
        "cardUpdateOptions": {
            "updateCardDataByKey": true
        }
    })
}

/// Build the DingTalk Advanced Card `btns` ButtonGroup JSON string.
///
/// The DingTalk `cardParamMap` requires every value to be a JSON string,
/// so this returns a serialized JSON array of the 5 canonical menu
/// actions exposed by the shared `AgentMenu`. Action keys are **never**
/// duplicated here — they are resolved from the shared `AgentMenuPanel`
/// so Feishu and DingTalk stay in lockstep on what the palette means.
///
/// Each button uses the standard `sendCardRequest` event so reactions
/// arrive on the Stream callback topic (`/v1.0/card/instances/callback`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DingtalkCardButtonGroups {
    pub btns: String,
    pub primary_actions: String,
    pub monitor_actions: String,
    pub help_actions: String,
}

fn build_card_button(action: &crate::gateway::agent_menu::AgentMenuAction) -> serde_json::Value {
    let color = match action.action {
        "gateway_status" | "monitor_30s" | "monitor_60s" => "blue",
        _ => "gray",
    };
    serde_json::json!({
        "text": action.label,
        "color": color,
        "status": "normal",
        "event": {
            "type": "sendCardRequest",
            "params": {
                "actionId": action.action,
                "params": {
                    "action": action.action
                }
            }
        }
    })
}

fn serialize_buttons(buttons: &[serde_json::Value]) -> String {
    serde_json::to_string(buttons).unwrap_or_else(|_| "[]".to_string())
}

pub fn build_card_button_groups() -> DingtalkCardButtonGroups {
    let panel = crate::gateway::agent_menu::build_agent_menu_panel();
    let actions = panel
        .primary_actions
        .iter()
        .chain(panel.secondary_actions)
        .collect::<Vec<_>>();
    let select = |keys: &[&str]| {
        keys.iter()
            .filter_map(|key| actions.iter().find(|action| action.action == *key))
            .map(|action| build_card_button(action))
            .collect::<Vec<_>>()
    };

    // DingTalk presentation slots only decide layout. Labels and canonical
    // actions continue to come from the cross-channel AgentMenu spec.
    let primary = select(&["gateway_status", "recent_jobs"]);
    let monitors = select(&["monitor_30s", "monitor_60s"]);
    let help = select(&["help"]);
    let legacy = primary
        .iter()
        .chain(monitors.iter())
        .chain(help.iter())
        .cloned()
        .collect::<Vec<_>>();

    DingtalkCardButtonGroups {
        btns: serialize_buttons(&legacy),
        primary_actions: serialize_buttons(&primary),
        monitor_actions: serialize_buttons(&monitors),
        help_actions: serialize_buttons(&help),
    }
}

pub fn build_card_buttons_json() -> String {
    build_card_button_groups().btns
}

pub fn card_status_label(status: &str) -> &'static str {
    match status {
        "READY" | "SUCCESS" => "在线",
        "RUNNING" => "执行中",
        "FAILED" => "上次操作失败",
        "BUSY" => "正在执行其他任务",
        _ => "状态未知",
    }
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
    let (target_kind, space_kind) = match target {
        DingtalkCardTarget::Group { .. } => ("group", "IM_GROUP"),
        DingtalkCardTarget::Direct { .. } => ("single", "IM_ROBOT"),
    };
    println!(
        "[dingtalk-card] create_start template_configured=true target_kind={} space_kind={} out_track_hash={}",
        target_kind,
        space_kind,
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
        "BUSY" => "BUSY",
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
    fn robot_conversation_type_1_is_direct() {
        assert_eq!(
            DingtalkRobotConversationType::parse("1").unwrap(),
            DingtalkRobotConversationType::Direct
        );
    }

    #[test]
    fn robot_conversation_type_2_is_group() {
        assert_eq!(
            DingtalkRobotConversationType::parse("2").unwrap(),
            DingtalkRobotConversationType::Group
        );
    }

    #[test]
    fn robot_conversation_type_numeric_2_is_group() {
        // Delivery-path drift: numeric 2 must normalize to Group the same way.
        let inbound = InboundMessage {
            channel: crate::channels::ChannelKind::Dingtalk,
            user_id: Some("user-test".to_string()),
            session_id: Some("group-conversation-test".to_string()),
            text: "menu".to_string(),
            metadata: std::collections::HashMap::from([
                ("conversationType".to_string(), serde_json::json!(2)),
                ("robotCode".to_string(), serde_json::json!("robot-test")),
                ("senderStaffId".to_string(), serde_json::json!("user-test")),
            ]),
        };
        let target =
            DingtalkCardTarget::from_inbound(&inbound, None).expect("group target must build");
        assert!(matches!(target, DingtalkCardTarget::Group { .. }));
        assert_eq!(
            target,
            DingtalkCardTarget::Group {
                open_conversation_id: "group-conversation-test".to_string(),
                robot_code: "robot-test".to_string(),
                user_id: Some("user-test".to_string()),
            }
        );
    }

    #[test]
    fn robot_conversation_type_unknown_is_error() {
        assert!(DingtalkRobotConversationType::parse("3").is_err());
        assert!(DingtalkRobotConversationType::parse("").is_err());
        assert!(DingtalkRobotConversationType::parse("group").is_err());
    }

    #[test]
    fn group_callback_builds_group_target_from_conversation_id() {
        // Realistic group robot callback: conversationType=2 must route the
        // card into the group (conversationId), never the sender's private chat.
        let inbound = InboundMessage {
            channel: crate::channels::ChannelKind::Dingtalk,
            user_id: Some("user-test".to_string()),
            session_id: Some("GROUP_A".to_string()),
            text: "menu".to_string(),
            metadata: std::collections::HashMap::from([
                ("conversationType".to_string(), serde_json::json!("2")),
                ("conversationId".to_string(), serde_json::json!("GROUP_A")),
                ("senderStaffId".to_string(), serde_json::json!("USER_A")),
                ("robotCode".to_string(), serde_json::json!("robot-test")),
            ]),
        };
        let target =
            DingtalkCardTarget::from_inbound(&inbound, None).expect("group target must build");
        let DingtalkCardTarget::Group {
            ref open_conversation_id,
            ref robot_code,
            ref user_id,
        } = target
        else {
            panic!("expected Group target");
        };
        assert_eq!(open_conversation_id, "GROUP_A");
        assert_eq!(robot_code, "robot-test");
        assert_eq!(user_id.as_deref(), Some("USER_A"));

        let payload = build_menu_create_payload("template", "track", &target);
        assert_eq!(
            payload["openSpaceId"].as_str().unwrap(),
            "dtv1.card//IM_GROUP.GROUP_A",
            "group card must use IM_GROUP space with conversationId"
        );
        assert!(
            !payload["openSpaceId"]
                .as_str()
                .unwrap()
                .contains("USER_A"),
            "group card must never use senderStaffId as space identity"
        );
    }

    #[test]
    fn group_callback_without_raw_payload_still_builds_group_target() {
        // Stream path carries conversationType at metadata top level, not
        // inside raw_payload. The builder must honor that.
        let inbound = InboundMessage {
            channel: crate::channels::ChannelKind::Dingtalk,
            user_id: Some("user-test".to_string()),
            session_id: Some("GROUP_B".to_string()),
            text: "menu".to_string(),
            metadata: std::collections::HashMap::from([
                ("conversationType".to_string(), serde_json::json!("2")),
                ("robotCode".to_string(), serde_json::json!("robot-test")),
            ]),
        };
        let target =
            DingtalkCardTarget::from_inbound(&inbound, None).expect("group target must build");
        let DingtalkCardTarget::Group {
            open_conversation_id, ..
        } = target
        else {
            panic!("expected Group target");
        };
        assert_eq!(open_conversation_id, "GROUP_B");
    }

    #[test]
    fn direct_callback_builds_direct_target_from_sender_staff_id() {
        // conversationType=1 must produce a Direct (IM_ROBOT) target whose
        // identity comes from senderStaffId.
        let inbound = InboundMessage {
            channel: crate::channels::ChannelKind::Dingtalk,
            user_id: Some("USER_A".to_string()),
            session_id: Some("DIRECT_SESSION".to_string()),
            text: "menu".to_string(),
            metadata: std::collections::HashMap::from([
                ("conversationType".to_string(), serde_json::json!("1")),
                ("conversationId".to_string(), serde_json::json!("DIRECT_SESSION")),
                ("senderStaffId".to_string(), serde_json::json!("USER_A")),
                ("robotCode".to_string(), serde_json::json!("robot-test")),
            ]),
        };
        let target =
            DingtalkCardTarget::from_inbound(&inbound, None).expect("direct target must build");
        assert_eq!(
            target,
            DingtalkCardTarget::Direct {
                user_id: "USER_A".to_string(),
                robot_code: "robot-test".to_string(),
            }
        );
        let payload = build_menu_create_payload("template", "track", &target);
        assert_eq!(
            payload["openSpaceId"].as_str().unwrap(),
            "dtv1.card//IM_ROBOT.USER_A"
        );
    }

    #[test]
    fn routing_identity_collision_group_wins_over_sender() {
        // conversationId=GROUP_A with senderStaffId=USER_A must build the
        // group space from GROUP_A. This is the regression guard for the
        // group-card-to-private-chat bug.
        let inbound = InboundMessage {
            channel: crate::channels::ChannelKind::Dingtalk,
            user_id: Some("USER_A".to_string()),
            session_id: Some("GROUP_A".to_string()),
            text: "menu".to_string(),
            metadata: std::collections::HashMap::from([
                ("conversationType".to_string(), serde_json::json!("2")),
                ("conversationId".to_string(), serde_json::json!("GROUP_A")),
                ("senderStaffId".to_string(), serde_json::json!("USER_A")),
                ("robotCode".to_string(), serde_json::json!("robot-test")),
            ]),
        };
        let target =
            DingtalkCardTarget::from_inbound(&inbound, None).expect("group target must build");
        let payload = build_menu_create_payload("template", "track", &target);
        let space = payload["openSpaceId"].as_str().unwrap();
        assert!(space.starts_with("dtv1.card//IM_GROUP.GROUP_A"), "space: {space}");
        assert!(!space.contains("USER_A"), "space must not use sender id: {space}");
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
        assert_eq!(payload["cardData"]["cardParamMap"]["status"], "在线");
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
        assert_eq!(payload["cardData"]["cardParamMap"]["status"], "在线");
    }

    #[test]
    fn opaque_id_never_contains_original_identifier() {
        let hash = opaque_id("secret-conversation-identifier");
        assert_eq!(hash.len(), 12);
        assert!(!hash.contains("secret"));
    }

    #[test]
    fn card_buttons_only_contain_canonical_actions() {
        let btns_str = build_card_buttons_json();
        let buttons: Vec<serde_json::Value> = serde_json::from_str(&btns_str).unwrap();
        assert_eq!(buttons.len(), 5, "btns must contain exactly 5 canonical actions");

        let allowed: std::collections::HashSet<&str> = [
            "gateway_status",
            "monitor_30s",
            "monitor_60s",
            "recent_jobs",
            "help",
        ]
        .iter()
        .copied()
        .collect();

        for btn in &buttons {
            // event.params.actionId is the canonical action
            let action_id = btn["event"]["params"]["actionId"]
                .as_str()
                .expect("actionId must be string");
            // event.params.params.action is the same canonical action
            let action = btn["event"]["params"]["params"]["action"]
                .as_str()
                .expect("params.action must be string");
            assert_eq!(action_id, action);
            assert!(
                allowed.contains(action),
                "btn contains non-canonical action {action:?}"
            );
            assert_eq!(btn["event"]["type"].as_str().unwrap(), "sendCardRequest");
            assert_eq!(btn["status"].as_str().unwrap(), "normal");
        }
    }

    #[test]
    fn card_buttons_match_dingtalk_template_order_and_labels() {
        let btns_str = build_card_buttons_json();
        let buttons: Vec<serde_json::Value> = serde_json::from_str(&btns_str).unwrap();

        // Required order: gateway_status, recent_jobs, monitor_30s,
        // monitor_60s, help — matches the DingTalk Card template slots.
        let expected_order = [
            ("gateway_status", "Gateway 状态", "blue"),
            ("recent_jobs", "最近任务", "gray"),
            ("monitor_30s", "桌面监控 30 秒", "blue"),
            ("monitor_60s", "桌面监控 60 秒", "blue"),
            ("help", "帮助说明", "gray"),
        ];
        for (i, (action, text, color)) in expected_order.iter().enumerate() {
            let btn = &buttons[i];
            assert_eq!(
                btn["event"]["params"]["actionId"].as_str().unwrap(),
                *action
            );
            assert_eq!(
                btn["event"]["params"]["params"]["action"].as_str().unwrap(),
                *action
            );
            assert_eq!(btn["text"].as_str().unwrap(), *text);
            assert_eq!(btn["color"].as_str().unwrap(), *color);
        }
    }

    #[test]
    fn card_buttons_payload_is_json_string() {
        // DingTalk cardParamMap values must be JSON strings.
        let btns_str = build_card_buttons_json();
        let parsed: serde_json::Value = serde_json::from_str(&btns_str).unwrap();
        assert!(parsed.is_array(), "btns must be a JSON array");
        assert_eq!(parsed.as_array().unwrap().len(), 5);
    }

    #[test]
    fn create_payload_includes_btns() {
        let target = DingtalkCardTarget::Direct {
            user_id: "user-secret".into(),
            robot_code: "robot-secret".into(),
        };
        let payload = build_menu_create_payload("template", "track", &target);
        let btns_str = payload["cardData"]["cardParamMap"]["btns"]
            .as_str()
            .expect("btns must be a JSON string in cardParamMap");
        let buttons: Vec<serde_json::Value> = serde_json::from_str(btns_str).unwrap();
        assert_eq!(buttons.len(), 5);
        assert_eq!(
            buttons[0]["event"]["params"]["actionId"].as_str().unwrap(),
            "gateway_status"
        );
    }

    #[test]
    fn update_payload_includes_btns() {
        let payload = build_card_update_payload(
            "track-xyz",
            "SUCCESS",
            "done",
            "status body",
            "gateway_status",
        );
        let btns_str = payload["cardData"]["cardParamMap"]["btns"]
            .as_str()
            .expect("update payload must also carry btns");
        let buttons: Vec<serde_json::Value> = serde_json::from_str(btns_str).unwrap();
        assert_eq!(buttons.len(), 5);
        // Every button must be a canonical action so the palette stays
        // intact across READY -> RUNNING -> SUCCESS updates.
        for btn in &buttons {
            let action = btn["event"]["params"]["actionId"]
                .as_str()
                .unwrap();
            assert!(
                matches!(
                    action,
                    "gateway_status" | "monitor_30s" | "monitor_60s" | "recent_jobs" | "help"
                ),
                "update payload button contains non-canonical action {action:?}"
            );
        }
    }

    #[test]
    fn create_and_update_share_the_same_btns_layout() {
        let target = DingtalkCardTarget::Direct {
            user_id: "user-secret".into(),
            robot_code: "robot-secret".into(),
        };
        let create = build_menu_create_payload("template", "track", &target);
        let update = build_card_update_payload("track", "RUNNING", "running", "x", "gateway_status");

        let create_btns = create["cardData"]["cardParamMap"]["btns"]
            .as_str()
            .unwrap();
        let update_btns = update["cardData"]["cardParamMap"]["btns"]
            .as_str()
            .unwrap();

        // Both payloads must emit an identical button list so the visible
        // palette does not disappear when the card enters RUNNING.
        assert_eq!(create_btns, update_btns);
    }

    #[test]
    fn create_payload_includes_two_two_one_button_groups() {
        let target = DingtalkCardTarget::Direct {
            user_id: "user-secret".into(),
            robot_code: "robot-secret".into(),
        };
        let payload = build_menu_create_payload("template", "track", &target);
        let params = &payload["cardData"]["cardParamMap"];
        let primary: Vec<serde_json::Value> =
            serde_json::from_str(params["primary_actions"].as_str().unwrap()).unwrap();
        let monitors: Vec<serde_json::Value> =
            serde_json::from_str(params["monitor_actions"].as_str().unwrap()).unwrap();
        let help: Vec<serde_json::Value> =
            serde_json::from_str(params["help_actions"].as_str().unwrap()).unwrap();
        assert_eq!(primary.len(), 2);
        assert_eq!(monitors.len(), 2);
        assert_eq!(help.len(), 1);
        assert_eq!(primary[0]["event"]["params"]["actionId"], "gateway_status");
        assert_eq!(primary[1]["event"]["params"]["actionId"], "recent_jobs");
        assert_eq!(monitors[0]["event"]["params"]["actionId"], "monitor_30s");
        assert_eq!(monitors[1]["event"]["params"]["actionId"], "monitor_60s");
        assert_eq!(help[0]["event"]["params"]["actionId"], "help");
    }

    #[test]
    fn update_payload_includes_the_same_group_fields_as_create() {
        let target = DingtalkCardTarget::Direct {
            user_id: "user-secret".into(),
            robot_code: "robot-secret".into(),
        };
        let create = build_menu_create_payload("template", "track", &target);
        let update = build_card_update_payload(
            "track",
            "RUNNING",
            "正在读取 Gateway 状态...",
            "正在读取 Gateway 状态...",
            "Gateway 状态",
        );
        for key in [
            "title",
            "status",
            "status_text",
            "result",
            "last_action",
            "btns",
            "primary_actions",
            "monitor_actions",
            "help_actions",
        ] {
            assert!(create["cardData"]["cardParamMap"][key].is_string());
            assert!(update["cardData"]["cardParamMap"][key].is_string());
        }
        for key in ["btns", "primary_actions", "monitor_actions", "help_actions"] {
            assert_eq!(
                create["cardData"]["cardParamMap"][key],
                update["cardData"]["cardParamMap"][key]
            );
        }
    }

    #[test]
    fn card_status_values_are_user_friendly() {
        assert_eq!(card_status_label("READY"), "在线");
        assert_eq!(card_status_label("RUNNING"), "执行中");
        assert_eq!(card_status_label("SUCCESS"), "在线");
        assert_eq!(card_status_label("FAILED"), "上次操作失败");
        assert_eq!(card_status_label("BUSY"), "正在执行其他任务");
    }
}
