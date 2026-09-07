use omninova_browser_host::desktop::handle_json_for_test;
use omninova_browser_host::protocol::TransportSession;
use serde_json::json;

fn session() -> TransportSession {
    TransportSession {
        connection_id: "conn-live".into(),
        generation: 11,
        transport_session_id: None,
        hello_completed: false,
    }
}

#[test]
fn hello_ping_capabilities_attach_detach_round_trip() {
    let mut session = session();
    let hello = handle_json_for_test(
        &json!({
            "protocol_version": 1,
            "request_id": "h1",
            "operation": "hello",
            "payload": { "protocol_version": 1, "extension_version": "0.1.0" }
        })
        .to_string(),
        &mut session,
    );
    assert_eq!(hello["ok"], true);
    assert_eq!(hello["request_id"], "h1");
    assert_eq!(hello["payload"]["kind"], "hello_ack");

    let ping = handle_json_for_test(
        &json!({
            "protocol_version": 1,
            "request_id": "p1",
            "operation": "ping",
            "payload": { "echo": "pong-me" }
        })
        .to_string(),
        &mut session,
    );
    assert_eq!(ping["request_id"], "p1");
    assert_eq!(ping["payload"]["echo"], "pong-me");

    let caps = handle_json_for_test(
        &json!({
            "protocol_version": 1,
            "request_id": "c1",
            "operation": "capabilities",
            "payload": {}
        })
        .to_string(),
        &mut session,
    );
    assert!(caps["payload"]["capabilities"].as_array().unwrap().len() >= 4);

    let attach = handle_json_for_test(
        &json!({
            "protocol_version": 1,
            "request_id": "a1",
            "operation": "attach_transport",
            "payload": {}
        })
        .to_string(),
        &mut session,
    );
    assert_eq!(attach["request_id"], "a1");
    assert_eq!(attach["payload"]["attached"], true);

    let detach = handle_json_for_test(
        &json!({
            "protocol_version": 1,
            "request_id": "d1",
            "operation": "detach_transport",
            "payload": {}
        })
        .to_string(),
        &mut session,
    );
    assert_eq!(detach["payload"]["attached"], false);
}

#[test]
fn version_mismatch_does_not_retry_into_hello() {
    let mut session = session();
    let response = handle_json_for_test(
        &json!({
            "protocol_version": 2,
            "request_id": "bad",
            "operation": "hello",
            "payload": { "protocol_version": 2 }
        })
        .to_string(),
        &mut session,
    );
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "ProtocolMismatch");
    assert!(!session.hello_completed);
}

#[test]
fn reconnect_is_a_fresh_transport_session() {
    let mut first = session();
    handle_json_for_test(
        &json!({
            "protocol_version": 1,
            "request_id": "h1",
            "operation": "hello",
            "payload": { "protocol_version": 1 }
        })
        .to_string(),
        &mut first,
    );
    handle_json_for_test(
        &json!({
            "protocol_version": 1,
            "request_id": "a1",
            "operation": "attach_transport",
            "payload": {}
        })
        .to_string(),
        &mut first,
    );
    assert!(first.transport_session_id.is_some());

    let mut reconnect = TransportSession {
        connection_id: "conn-live-2".into(),
        generation: 12,
        transport_session_id: None,
        hello_completed: false,
    };
    let hello = handle_json_for_test(
        &json!({
            "protocol_version": 1,
            "request_id": "h2",
            "operation": "hello",
            "payload": { "protocol_version": 1 }
        })
        .to_string(),
        &mut reconnect,
    );
    assert_eq!(hello["payload"]["connection_id"], "conn-live-2");
    assert_ne!(
        hello["payload"]["generation"],
        serde_json::json!(first.generation)
    );
    assert!(reconnect.transport_session_id.is_none());
}
