#![cfg(windows)]

use std::time::Duration;

use omninova_browser_host::desktop::PersonalChromeBridge;
use omninova_browser_host::framing::{read_frame, write_frame};
use omninova_browser_host::ipc::AuthRequest;
use omninova_browser_host::secret::{load_endpoint, load_secret};
use serde_json::json;
use tokio::net::windows::named_pipe::ClientOptions;

async fn connect_retry(path: &str) -> tokio::net::windows::named_pipe::NamedPipeClient {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match ClientOptions::new().open(path) {
            Ok(client) => return client,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(err) => panic!("named pipe connect failed: {err}"),
        }
    }
}

#[tokio::test]
async fn named_pipe_auth_hello_ping_reconnect_and_stale_generation() {
    let tmp = tempfile::tempdir().unwrap();
    let bridge = PersonalChromeBridge::spawn(tmp.path().to_path_buf()).unwrap();
    let endpoint = load_endpoint(tmp.path()).unwrap();
    let secret = load_secret(tmp.path()).unwrap();
    assert_eq!(endpoint.transport, "named_pipe");
    assert!(endpoint.path.starts_with(r"\\.\pipe\"));

    let mut client = connect_retry(&endpoint.path).await;
    let auth = AuthRequest {
        protocol_version: 1,
        secret: secret.as_str().to_string(),
        generation: endpoint.generation,
        connection_nonce: endpoint.connection_nonce.clone(),
        host_pid: std::process::id(),
    };
    write_frame(&mut client, &serde_json::to_string(&auth).unwrap())
        .await
        .unwrap();
    let ack = read_frame(&mut client).await.unwrap().unwrap();
    assert!(ack.contains("\"ok\":true"), "{ack}");
    assert!(!ack.contains(secret.as_str()));

    write_frame(
        &mut client,
        &json!({
            "protocol_version": 1,
            "request_id": "h1",
            "operation": "hello",
            "payload": { "protocol_version": 1, "extension_version": "0.1.0" }
        })
        .to_string(),
    )
    .await
    .unwrap();
    let hello = read_frame(&mut client).await.unwrap().unwrap();
    assert!(hello.contains("hello_ack"));
    assert!(hello.contains("\"request_id\":\"h1\""));

    write_frame(
        &mut client,
        &json!({
            "protocol_version": 1,
            "request_id": "p1",
            "operation": "ping",
            "payload": { "echo": "live" }
        })
        .to_string(),
    )
    .await
    .unwrap();
    let pong = read_frame(&mut client).await.unwrap().unwrap();
    assert!(pong.contains("\"request_id\":\"p1\""));
    assert!(pong.contains("live"));

    drop(client);
    tokio::time::sleep(Duration::from_millis(150)).await;

    let mut client = connect_retry(&endpoint.path).await;
    write_frame(&mut client, &serde_json::to_string(&auth).unwrap())
        .await
        .unwrap();
    let ack2 = read_frame(&mut client).await.unwrap().unwrap();
    assert!(ack2.contains("\"ok\":true"));
    drop(client);

    bridge.rotate_ipc_secret().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let new_endpoint = load_endpoint(tmp.path()).unwrap();
    assert_ne!(new_endpoint.generation, endpoint.generation);

    let mut stale = connect_retry(&new_endpoint.path).await;
    write_frame(&mut stale, &serde_json::to_string(&auth).unwrap())
        .await
        .unwrap();
    let rejected = read_frame(&mut stale).await.unwrap().unwrap();
    assert!(
        rejected.contains("StaleGeneration") || rejected.contains("AuthenticationFailed"),
        "{rejected}"
    );
    assert!(!rejected.contains(secret.as_str()));
}

#[tokio::test]
async fn desktop_request_is_forwarded_and_correlated() {
    let tmp = tempfile::tempdir().unwrap();
    let bridge = PersonalChromeBridge::spawn(tmp.path().to_path_buf()).unwrap();
    let endpoint = load_endpoint(tmp.path()).unwrap();
    let secret = load_secret(tmp.path()).unwrap();
    let mut client = connect_retry(&endpoint.path).await;
    let auth = AuthRequest {
        protocol_version: 1,
        secret: secret.as_str().to_string(),
        generation: endpoint.generation,
        connection_nonce: endpoint.connection_nonce.clone(),
        host_pid: std::process::id(),
    };
    write_frame(&mut client, &serde_json::to_string(&auth).unwrap())
        .await
        .unwrap();
    let _ack = read_frame(&mut client).await.unwrap().unwrap();
    write_frame(
        &mut client,
        &json!({
            "protocol_version": 1,
            "request_id": "h1",
            "operation": "hello",
            "payload": { "protocol_version": 1, "extension_version": "0.1.0" }
        })
        .to_string(),
    )
    .await
    .unwrap();
    let hello = read_frame(&mut client).await.unwrap().unwrap();
    assert!(hello.contains("hello_ack"));

    let bridge2 = bridge.clone();
    let responder = tokio::spawn(async move {
        let req = read_frame(&mut client).await.unwrap().unwrap();
        assert!(req.contains("tab_list_authorized"));
        assert!(!req.contains(secret.as_str()));
        let parsed: serde_json::Value = serde_json::from_str(&req).unwrap();
        let id = parsed["request_id"].as_str().unwrap().to_string();
        write_frame(
            &mut client,
            &json!({
                "protocol_version": 1,
                "request_id": id,
                "ok": true,
                "payload": { "tabs": [] }
            })
            .to_string(),
        )
        .await
        .unwrap();
        client
    });

    let response = tokio::time::timeout(
        Duration::from_secs(5),
        bridge2.request("tab_list_authorized", "", json!({})),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(response.ok);
    assert_eq!(response.payload.unwrap()["tabs"], json!([]));
    let _ = responder.await.unwrap();
}
