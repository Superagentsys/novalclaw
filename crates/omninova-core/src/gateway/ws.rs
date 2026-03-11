use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use futures_util::SinkExt;
use serde::{Deserialize, Serialize};

use crate::channels::{ChannelKind, InboundMessage};
use crate::gateway::GatewayRuntime;

#[derive(Debug, Deserialize)]
pub struct WsChatQuery {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WsClientMessage {
    #[serde(rename = "type")]
    msg_type: String,
    content: Option<String>,
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct WsServerMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    full_response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent: Option<String>,
}

pub async fn ws_chat_handler(
    ws: WebSocketUpgrade,
    State(runtime): State<GatewayRuntime>,
    Query(query): Query<WsChatQuery>,
) -> impl IntoResponse {
    let cfg = runtime.get_config().await;
    if cfg.gateway.require_pairing {
        if let Some(token) = &query.token {
            if !verify_pairing_token(&cfg, token) {
                return ws.on_upgrade(|mut socket: WebSocket| async move {
                    let msg = serde_json::to_string(&WsServerMessage {
                        msg_type: "error".to_string(),
                        full_response: None,
                        message: Some("unauthorized".to_string()),
                        agent: None,
                    })
                    .unwrap_or_default();
                    let _ = socket.send(Message::Text(msg.into())).await;
                    let _ = socket.close().await;
                });
            }
        }
    }
    ws.on_upgrade(move |socket| handle_ws_session(socket, runtime))
}

async fn handle_ws_session(mut socket: WebSocket, runtime: GatewayRuntime) {
    while let Some(Ok(msg)) = socket.recv().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let client_msg: WsClientMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                let err = serde_json::to_string(&WsServerMessage {
                    msg_type: "error".to_string(),
                    full_response: None,
                    message: Some(format!("invalid message: {e}")),
                    agent: None,
                })
                .unwrap_or_default();
                let _ = socket.send(Message::Text(err.into())).await;
                continue;
            }
        };

        if client_msg.msg_type != "message" {
            continue;
        }

        let content = client_msg.content.unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }

        let inbound = InboundMessage {
            channel: ChannelKind::Web,
            user_id: Some("ws-user".to_string()),
            session_id: client_msg.session_id,
            text: content,
            metadata: Default::default(),
        };

        let response = match runtime.process_inbound(&inbound).await {
            Ok(resp) => WsServerMessage {
                msg_type: "done".to_string(),
                full_response: Some(resp.reply),
                message: None,
                agent: Some(resp.route.agent_name),
            },
            Err(e) => WsServerMessage {
                msg_type: "error".to_string(),
                full_response: None,
                message: Some(e.to_string()),
                agent: None,
            },
        };

        let payload = serde_json::to_string(&response).unwrap_or_default();
        if socket.send(Message::Text(payload.into())).await.is_err() {
            break;
        }
    }
}

fn verify_pairing_token(cfg: &crate::config::Config, _token: &str) -> bool {
    let _ = cfg;
    true
}
