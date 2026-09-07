use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::constants::{protocol_version, TRANSPORT_CAPABILITIES};
use crate::error::BridgeError;

pub const OP_HELLO: &str = "hello";
pub const OP_PING: &str = "ping";
pub const OP_CAPABILITIES: &str = "capabilities";
pub const OP_ATTACH: &str = "attach_transport";
pub const OP_DETACH: &str = "detach_transport";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportRequest {
    pub protocol_version: u32,
    pub request_id: String,
    #[serde(default)]
    pub session_id: String,
    pub operation: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportResponse {
    pub protocol_version: u32,
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TransportErrorBody>,
}

impl TransportResponse {
    pub fn ok(request_id: impl Into<String>, payload: Value) -> Self {
        Self {
            protocol_version: protocol_version(),
            request_id: request_id.into(),
            ok: true,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn err(request_id: impl Into<String>, err: &BridgeError) -> Self {
        Self {
            protocol_version: protocol_version(),
            request_id: request_id.into(),
            ok: false,
            payload: None,
            error: Some(TransportErrorBody {
                code: err.code().to_string(),
                message: err.to_string(),
            }),
        }
    }
}

#[derive(Debug, Default)]
pub struct TransportSession {
    pub connection_id: String,
    pub generation: u64,
    pub transport_session_id: Option<String>,
    pub hello_completed: bool,
}

pub fn parse_request(raw: &str) -> Result<TransportRequest, BridgeError> {
    serde_json::from_str(raw).map_err(|err| BridgeError::MalformedFrame {
        detail: err.to_string(),
    })
}

pub fn dispatch(session: &mut TransportSession, req: &TransportRequest) -> TransportResponse {
    if req.protocol_version != protocol_version() {
        return TransportResponse::err(
            &req.request_id,
            &BridgeError::ProtocolMismatch {
                requested: req.protocol_version,
                expected: protocol_version(),
            },
        );
    }

    match req.operation.as_str() {
        OP_HELLO => hello(session, req),
        OP_PING => ping(req),
        OP_CAPABILITIES => capabilities(req),
        OP_ATTACH => attach(session, req),
        OP_DETACH => detach(session, req),
        other => TransportResponse::err(
            &req.request_id,
            &BridgeError::UnknownOperation {
                operation: other.to_string(),
            },
        ),
    }
}

fn hello(session: &mut TransportSession, req: &TransportRequest) -> TransportResponse {
    let payload_version = req
        .payload
        .get("protocol_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(req.protocol_version as u64) as u32;
    if payload_version != protocol_version() {
        return TransportResponse::err(
            &req.request_id,
            &BridgeError::ProtocolMismatch {
                requested: payload_version,
                expected: protocol_version(),
            },
        );
    }
    session.hello_completed = true;
    TransportResponse::ok(
        &req.request_id,
        json!({
            "kind": "hello_ack",
            "protocol_version": protocol_version(),
            "capabilities": TRANSPORT_CAPABILITIES,
            "connection_id": session.connection_id,
            "generation": session.generation,
        }),
    )
}

fn ping(req: &TransportRequest) -> TransportResponse {
    TransportResponse::ok(
        &req.request_id,
        json!({
            "kind": "pong",
            "echo": req.payload.get("echo").cloned().unwrap_or(Value::Null),
        }),
    )
}

fn capabilities(req: &TransportRequest) -> TransportResponse {
    TransportResponse::ok(
        &req.request_id,
        json!({
            "protocol_version": protocol_version(),
            "capabilities": TRANSPORT_CAPABILITIES,
        }),
    )
}

fn attach(session: &mut TransportSession, req: &TransportRequest) -> TransportResponse {
    let id = format!("transport:{}", uuid::Uuid::new_v4());
    session.transport_session_id = Some(id.clone());
    TransportResponse::ok(
        &req.request_id,
        json!({
            "attached": true,
            "transport_session_id": id,
            "connection_id": session.connection_id,
            "generation": session.generation,
        }),
    )
}

fn detach(session: &mut TransportSession, req: &TransportRequest) -> TransportResponse {
    session.transport_session_id = None;
    TransportResponse::ok(
        &req.request_id,
        json!({
            "attached": false,
            "connection_id": session.connection_id,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> TransportSession {
        TransportSession {
            connection_id: "conn-1".into(),
            generation: 7,
            transport_session_id: None,
            hello_completed: false,
        }
    }

    fn req(op: &str, version: u32, id: &str, payload: Value) -> TransportRequest {
        TransportRequest {
            protocol_version: version,
            request_id: id.into(),
            session_id: String::new(),
            operation: op.into(),
            payload,
        }
    }

    #[test]
    fn hello_ack_round_trips_request_id_and_capabilities() {
        let mut s = session();
        let response = dispatch(
            &mut s,
            &req(OP_HELLO, 1, "req-hello", json!({ "protocol_version": 1, "extension_version": "0.1.0" })),
        );
        assert!(response.ok);
        assert_eq!(response.request_id, "req-hello");
        assert_eq!(response.payload.unwrap()["kind"], "hello_ack");
        assert!(s.hello_completed);
    }

    #[test]
    fn protocol_mismatch_fails_closed() {
        let mut s = session();
        let response = dispatch(&mut s, &req(OP_HELLO, 99, "req-bad", json!({})));
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "ProtocolMismatch");
        assert!(!s.hello_completed);
    }

    #[test]
    fn payload_protocol_mismatch_fails_closed() {
        let mut s = session();
        let response = dispatch(
            &mut s,
            &req(OP_HELLO, 1, "req-bad-payload", json!({ "protocol_version": 2 })),
        );
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "ProtocolMismatch");
    }

    #[test]
    fn unknown_operation_is_typed() {
        let mut s = session();
        let response = dispatch(&mut s, &req("snapshot", 1, "req-x", json!({})));
        assert_eq!(response.error.unwrap().code, "UnknownOperation");
    }

    #[test]
    fn out_of_order_responses_keep_request_ids() {
        let mut s = session();
        let a = dispatch(&mut s, &req(OP_PING, 1, "a", json!({"echo": 1})));
        let b = dispatch(&mut s, &req(OP_CAPABILITIES, 1, "b", json!({})));
        assert_eq!(a.request_id, "a");
        assert_eq!(b.request_id, "b");
        assert_eq!(a.payload.unwrap()["echo"], 1);
        assert!(b.payload.unwrap()["capabilities"].is_array());
    }

    #[test]
    fn attach_is_not_a_browser_session_key() {
        let mut s = session();
        let response = dispatch(&mut s, &req(OP_ATTACH, 1, "att", json!({})));
        let id = response.payload.unwrap()["transport_session_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(id.starts_with("transport:"));
        assert_ne!(id, s.connection_id);
        let detached = dispatch(&mut s, &req(OP_DETACH, 1, "det", json!({})));
        assert_eq!(detached.payload.unwrap()["attached"], false);
        assert!(s.transport_session_id.is_none());
    }
}
