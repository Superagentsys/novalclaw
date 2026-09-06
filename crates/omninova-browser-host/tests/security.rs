use std::process::{Command, Stdio};

use omninova_browser_host::constants::{
    application_max_message_bytes, native_host_name, protocol_version, TRANSPORT_CAPABILITIES,
};
use omninova_browser_host::framing::{decode_reader, encode_raw_json};
use omninova_browser_host::ipc::{verify_auth, AuthRequest};
use omninova_browser_host::origin::verify_connecting_origin;
use omninova_browser_host::protocol::{dispatch, parse_request, TransportSession};
use omninova_browser_host::secret::Secret;
use omninova_browser_host::BridgeError;

#[test]
fn wrong_extension_origin_is_rejected() {
    let err = verify_connecting_origin(["host", "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/"])
        .unwrap_err();
    assert!(matches!(err, BridgeError::OriginRejected));
}

#[test]
fn malformed_native_messaging_frame_is_rejected() {
    let mut cur = std::io::Cursor::new([3, 0, 0, 0, 0xFF, 0xFE, 0xFD]);
    let err = decode_reader(&mut cur).unwrap_err();
    assert!(
        matches!(err, BridgeError::MalformedFrame { .. }),
        "got {err:?}"
    );
}

#[test]
fn oversized_frame_and_application_payload_are_rejected() {
    let too_big = "n".repeat(application_max_message_bytes() + 8);
    assert!(matches!(
        encode_raw_json(&too_big),
        Err(BridgeError::PayloadTooLarge { .. })
    ));
    let mut claimed = (application_max_message_bytes() as u32 + 4).to_ne_bytes().to_vec();
    claimed.extend_from_slice(&[b'x'; 16]);
    let err = decode_reader(&mut std::io::Cursor::new(claimed)).unwrap_err();
    assert!(matches!(err, BridgeError::PayloadTooLarge { .. }));
}

#[test]
fn unknown_operation_and_protocol_mismatch_are_typed() {
    let mut session = TransportSession {
        connection_id: "c".into(),
        generation: 1,
        transport_session_id: None,
        hello_completed: false,
    };
    let unknown = parse_request(
        r#"{"protocol_version":1,"request_id":"1","operation":"eval","payload":{}}"#,
    )
    .unwrap();
    let unknown_res = dispatch(&mut session, &unknown);
    assert_eq!(unknown_res.error.unwrap().code, "UnknownOperation");
    let mismatch = parse_request(
        r#"{"protocol_version":9,"request_id":"2","operation":"hello","payload":{}}"#,
    )
    .unwrap();
    let mismatch_res = dispatch(&mut session, &mismatch);
    assert_eq!(mismatch_res.error.unwrap().code, "ProtocolMismatch");
}

#[test]
fn missing_and_wrong_secret_and_stale_generation() {
    let secret = Secret::from_raw("transport-secret-transport-secret".into());
    let missing = AuthRequest {
        protocol_version: 1,
        secret: String::new(),
        generation: 1,
        connection_nonce: "n".into(),
        host_pid: 1,
    };
    assert!(matches!(
        verify_auth(&missing, &secret, 1, "n", 1),
        Err(BridgeError::MissingSecret)
    ));
    let mut wrong = missing;
    wrong.secret = "other".into();
    let err = verify_auth(&wrong, &secret, 1, "n", 1).unwrap_err();
    assert!(matches!(err, BridgeError::AuthenticationFailed));
    assert!(!format!("{err:?}").contains("other"));
    assert!(!format!("{err:?}").contains("transport-secret"));
    wrong.secret = secret.as_str().to_string();
    wrong.generation = 1;
    let stale = verify_auth(&wrong, &secret, 4, "fresh", 1).unwrap_err();
    assert!(matches!(stale, BridgeError::StaleGeneration { .. }));
}

#[test]
fn stdout_discipline_source_guard() {
    let main = include_str!("../src/main.rs");
    let host = include_str!("../src/host_loop.rs");
    assert!(!main.lines().any(|line| line.contains("println!") && !line.contains("eprintln!")));
    assert!(!host.lines().any(|line| line.contains("println!") && !line.contains("eprintln!")));
    assert!(host.contains("write_stdout_frame"));
}

#[test]
fn native_host_binary_origin_rejection_does_not_print_protocol_to_stdout() {
    let exe = env!("CARGO_BIN_EXE_omninova-browser-host");
    let output = Command::new(exe)
        .arg("chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn host");
    assert_ne!(output.status.code(), Some(0));
    assert!(
        output.stdout.is_empty(),
        "rejected origin must not write Native Messaging frames: {:?}",
        output.stdout
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("OriginRejected") || stderr.contains("omninova-browser-host"));
    assert!(!stderr.contains("ipc.secret"));
}

#[test]
fn capabilities_are_transport_only() {
    assert!(!TRANSPORT_CAPABILITIES.iter().any(|c| {
        matches!(
            *c,
            "snapshot" | "click" | "fill" | "eval" | "cookies" | "storage" | "downloads"
        )
    }));
    assert_eq!(protocol_version(), 1);
    assert_eq!(native_host_name(), "com.omninova.browser_host");
}

#[test]
fn host_binary_missing_origin_exits_without_stdout_frames() {
    let exe = env!("CARGO_BIN_EXE_omninova-browser-host");
    let output = Command::new(exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn host");
    assert!(output.stdout.is_empty());
    assert_ne!(output.status.code(), Some(0));
}
