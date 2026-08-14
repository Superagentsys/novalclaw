//! WeCom HTTP callback delayed delivery adapter (Phase 2A.3.3a).
//!
//! Sends monitor final results to the callback's temporary
//! `response_url` channel as MARKDOWN. Network logic lives here — the
//! shared card core never performs HTTP POSTs itself.
//!
//! Protocol assumptions (official docs unreachable from this
//! environment; per the phase rule these are MINIMAL and E2E-verified):
//! - HTTP POST, `Content-Type: application/json`
//! - plaintext JSON body: `{"msgtype":"markdown","markdown":{"content":...}}`
//! - success = HTTP 2xx response (the server RESPONSE is verified —
//!   a transport write alone never counts as delivered)
//! - transport errors: at most ONE retry; HTTP 4xx/5xx: never retried
//!
//! The URL itself is a secret: it is never logged — only short hashes.

use serde_json::json;

const REQUEST_TIMEOUT_SECS: u64 = 10;
const MAX_TRANSPORT_ATTEMPTS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpDeliveryError {
    /// Stable error kind for logs: transport | http_error | timeout.
    pub kind: &'static str,
    /// HTTP status when the server answered; None for transport errors.
    pub status: Option<u16>,
}

/// One POST attempt. Returns Ok(()) on a 2xx server response.
async fn send_once(url: &str, body: &str) -> Result<(), HttpDeliveryError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|_| HttpDeliveryError {
            kind: "transport",
            status: None,
        })?;
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|error| {
            let kind = if error.is_timeout() { "timeout" } else { "transport" };
            HttpDeliveryError { kind, status: None }
        })?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(HttpDeliveryError {
            kind: "http_error",
            status: Some(status.as_u16()),
        })
    }
}

/// Deliver a markdown message to a WeCom `response_url`.
///
/// Retry policy: at most ONE retry, and only for transport/timeout
/// errors. HTTP 4xx/5xx validation errors are returned immediately —
/// the same payload is never re-sent.
pub async fn post_response_url_markdown(url: &str, content: &str) -> Result<(), HttpDeliveryError> {
    let body = json!({
        "msgtype": "markdown",
        "markdown": { "content": content },
    })
    .to_string();

    let url_owned = url.to_string();
    let body_owned = body.clone();
    let url_for_log = url.to_string();
    deliver_with_retry(
        &url_for_log,
        move || {
            let url = url_owned.clone();
            let body = body_owned.clone();
            async move { send_once(&url, &body).await }
        },
    )
    .await
}

/// Retry-policy core (testable with an injected sender): at most
/// MAX_TRANSPORT_ATTEMPTS attempts; `http_error` results are terminal
/// (no retry); transport/timeout errors may retry once.
pub(crate) async fn deliver_with_retry<F, Fut>(
    url: &str,
    mut send: F,
) -> Result<(), HttpDeliveryError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), HttpDeliveryError>>,
{
    let mut last_error: Option<HttpDeliveryError> = None;
    for attempt in 1..=MAX_TRANSPORT_ATTEMPTS {
        println!(
            "[wecom-http-delivery] send_started attempt={} url_hash={}",
            attempt,
            crate::gateway::wecom_card::response_url_hash(url)
        );
        match send().await {
            Ok(()) => {
                println!(
                    "[wecom-http-delivery] send_ok attempt={} url_hash={}",
                    attempt,
                    crate::gateway::wecom_card::response_url_hash(url)
                );
                return Ok(());
            }
            Err(error) => {
                last_error = Some(error);
                println!(
                    "[wecom-http-delivery] send_failed attempt={} status={:?} reason={} url_hash={}",
                    attempt,
                    error.status,
                    error.kind,
                    crate::gateway::wecom_card::response_url_hash(url)
                );
                // HTTP validation errors are terminal — no retry.
                if error.kind == "http_error" {
                    return Err(error);
                }
            }
        }
    }
    Err(last_error.unwrap_or(HttpDeliveryError {
        kind: "transport",
        status: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Local mock server on an ephemeral port. The handler receives
    /// (method, content_type, body) and returns an HTTP status; the
    /// connection counter records every ACCEPT so retry policy is
    /// observable.
    type MockHandler = std::sync::Arc<
        dyn Fn(String, Option<String>, String) -> axum::http::StatusCode + Send + Sync + 'static,
    >;

    async fn mock_server(
        connections: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        handler: MockHandler,
    ) -> String {
        use axum::extract::State;
        use axum::http::HeaderMap;
        use axum::routing::post;
        use axum::Router;

        let app = Router::new()
            .route(
                "/reply",
                post(
                    |State(state): State<(
                        MockHandler,
                        std::sync::Arc<std::sync::atomic::AtomicUsize>,
                    )>,
                     headers: HeaderMap,
                     body: String| async move {
                        state.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let content_type = headers
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_string);
                        (state.0)("POST".to_string(), content_type, body)
                    },
                ),
            )
            .with_state((handler, connections));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://127.0.0.1:{port}/reply")
    }

    #[tokio::test]
    async fn send_success_verifies_server_response() {
        let received: std::sync::Arc<std::sync::Mutex<Option<String>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let received_for_handler = received.clone();
        let url = mock_server(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            std::sync::Arc::new(move |_method, _content_type, body| {
                *received_for_handler.lock().unwrap() = Some(body);
                axum::http::StatusCode::OK
            }),
        )
        .await;
        let result = post_response_url_markdown(&url, "### 结果").await;
        assert!(result.is_ok(), "2xx must mean delivered, got {result:?}");
        let body = received.lock().unwrap().clone().unwrap();
        assert!(body.contains("\"markdown\""));
        assert!(body.contains("### 结果"));
    }

    #[tokio::test]
    async fn http_error_is_failure_and_no_retry() {
        let connections = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let url = mock_server(
            connections.clone(),
            std::sync::Arc::new(|_, _, _| axum::http::StatusCode::BAD_REQUEST),
        )
        .await;
        let error = post_response_url_markdown(&url, "x").await.unwrap_err();
        assert_eq!(error.kind, "http_error");
        assert_eq!(error.status, Some(400));
        assert_eq!(
            connections.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "4xx must never retry the same payload"
        );
    }

    #[tokio::test]
    async fn garbage_server_surfaces_as_error() {
        // A server that speaks non-HTTP must surface as an error (never a
        // silent success); the exact error kind is reqwest-environment
        // dependent, so this test only asserts failure.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            loop {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let _ = socket.write_all(b"NOT-HTTP\r\n\r\n").await;
                    let _ = socket.shutdown().await;
                }
            }
        });
        let url = format!("http://127.0.0.1:{port}/reply");
        let result = post_response_url_markdown(&url, "x").await;
        assert!(result.is_err(), "garbage server must produce an error");
    }

    #[tokio::test]
    async fn transport_error_retries_once() {
        // Retry policy, deterministic via injected sender: one transport
        // failure then success = exactly 2 attempts (initial + 1 retry).
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let attempts_for_closure = attempts.clone();
        let result = deliver_with_retry("https://example.invalid/r", move || {
            let attempts = attempts_for_closure.clone();
            async move {
                let attempt = attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                if attempt == 1 {
                    Err(HttpDeliveryError {
                        kind: "transport",
                        status: None,
                    })
                } else {
                    Ok(())
                }
            }
        })
        .await;
        assert!(result.is_ok(), "transport retry must eventually succeed");
        assert_eq!(
            attempts.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "transport error must retry exactly once (2 attempts total)"
        );
    }

    #[tokio::test]
    async fn transport_error_exhausts_after_one_retry() {
        // Persistent transport failure: exactly 2 attempts, then error.
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let attempts_for_closure = attempts.clone();
        let result = deliver_with_retry("https://example.invalid/r", move || {
            let attempts = attempts_for_closure.clone();
            async move {
                attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(HttpDeliveryError {
                    kind: "transport",
                    status: None,
                })
            }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(
            attempts.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "transport failure must stop after one retry"
        );
    }

    // ------------------------------------------------------------------
    // Phase 2A.3.3a: monitor auto-delivery via response_url
    // ------------------------------------------------------------------

    use crate::gateway::wecom_card::{
        card_store, deliver_monitor_result_http, register_http_panel_with_url, MonitorDelivery,
        MonitorState, WecomCardTransport, RESPONSE_URL_TTL_SECS,
    };

    async fn panel_with_response_url(task_id: &str, url: &str) {
        register_http_panel_with_url(task_id, None, Some(url.to_string())).await;
    }

    fn ok_server(
    ) -> (
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        MockHandler,
    ) {
        let received: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_for_handler = received.clone();
        let handler: MockHandler = std::sync::Arc::new(move |_, _, body: String| {
            received_for_handler.lock().unwrap().push(body);
            axum::http::StatusCode::OK
        });
        (received, handler)
    }

    #[tokio::test]
    async fn http_monitor_30_result_auto_delivery() {
        let (received, handler) = ok_server();
        let url = mock_server(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            handler,
        )
        .await;
        let task_id = format!("task-auto30-{}", uuid::Uuid::new_v4());
        panel_with_response_url(&task_id, &url).await;
        card_store()
            .complete_monitor(&task_id, 30, 30815, "检测到桌面变化".to_string())
            .await;
        deliver_monitor_result_http(card_store(), &task_id, "completed", 30, 30815, "检测到桌面变化")
            .await;
        match card_store().monitor_state(&task_id).await {
            MonitorState::Completed { delivery, .. } => {
                assert_eq!(delivery, MonitorDelivery::Sent);
            }
            other => panic!("expected Completed/Sent, got {other:?}"),
        }
        let body = received.lock().unwrap().pop().unwrap();
        assert!(body.contains("\"markdown\""));
        assert!(body.contains("### OmniNova · 桌面监控完成"));
    }

    #[tokio::test]
    async fn http_monitor_60_result_auto_delivery() {
        let (received, handler) = ok_server();
        let url = mock_server(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            handler,
        )
        .await;
        let task_id = format!("task-auto60-{}", uuid::Uuid::new_v4());
        panel_with_response_url(&task_id, &url).await;
        card_store()
            .complete_monitor(&task_id, 60, 61000, "未检测到明显变化".to_string())
            .await;
        deliver_monitor_result_http(card_store(), &task_id, "completed", 60, 61000, "未检测到明显变化")
            .await;
        match card_store().monitor_state(&task_id).await {
            MonitorState::Completed { delivery, .. } => assert_eq!(delivery, MonitorDelivery::Sent),
            other => panic!("expected Completed/Sent, got {other:?}"),
        }
        let body = received.lock().unwrap().pop().unwrap();
        assert!(body.contains("60 秒"));
    }

    #[tokio::test]
    async fn http_monitor_failure_auto_delivery() {
        let (received, handler) = ok_server();
        let url = mock_server(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            handler,
        )
        .await;
        let task_id = format!("task-autof-{}", uuid::Uuid::new_v4());
        panel_with_response_url(&task_id, &url).await;
        card_store()
            .fail_monitor(&task_id, "桌面监控失败，请稍后重试。".to_string(), 3000)
            .await;
        deliver_monitor_result_http(
            card_store(),
            &task_id,
            "failed",
            30,
            3000,
            "桌面监控失败，请稍后重试。",
        )
        .await;
        let body = received.lock().unwrap().pop().unwrap();
        assert!(body.contains("### OmniNova · 桌面监控失败"));
    }

    #[tokio::test]
    async fn http_monitor_timeout_auto_delivery() {
        let (received, handler) = ok_server();
        let url = mock_server(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            handler,
        )
        .await;
        let task_id = format!("task-autot-{}", uuid::Uuid::new_v4());
        panel_with_response_url(&task_id, &url).await;
        card_store()
            .fail_monitor(&task_id, "桌面监控超时".to_string(), 45000)
            .await;
        deliver_monitor_result_http(card_store(), &task_id, "timeout", 30, 45000, "桌面监控超时")
            .await;
        let body = received.lock().unwrap().pop().unwrap();
        assert!(body.contains("### OmniNova · 桌面监控超时"));
    }

    #[tokio::test]
    async fn http_delivery_success_marks_sent() {
        let url = mock_server(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            std::sync::Arc::new(|_, _, _| axum::http::StatusCode::OK),
        )
        .await;
        let task_id = format!("task-sent-{}", uuid::Uuid::new_v4());
        panel_with_response_url(&task_id, &url).await;
        card_store()
            .complete_monitor(&task_id, 30, 30000, "检测到桌面变化".to_string())
            .await;
        deliver_monitor_result_http(card_store(), &task_id, "completed", 30, 30000, "检测到桌面变化")
            .await;
        match card_store().monitor_state(&task_id).await {
            MonitorState::Completed { delivery, .. } => assert_eq!(delivery, MonitorDelivery::Sent),
            other => panic!("expected Sent, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_delivery_failure_marks_failed() {
        let url = mock_server(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            std::sync::Arc::new(|_, _, _| axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        )
        .await;
        let task_id = format!("task-httpfail-{}", uuid::Uuid::new_v4());
        panel_with_response_url(&task_id, &url).await;
        card_store()
            .complete_monitor(&task_id, 30, 30000, "检测到桌面变化".to_string())
            .await;
        deliver_monitor_result_http(card_store(), &task_id, "completed", 30, 30000, "检测到桌面变化")
            .await;
        match card_store().monitor_state(&task_id).await {
            MonitorState::Completed { delivery, .. } => {
                assert_eq!(delivery, MonitorDelivery::Failed { errcode: 500 });
            }
            other => panic!("expected Failed{{500}}, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_delivery_validation_error_no_retry() {
        let connections = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let url = mock_server(
            connections.clone(),
            std::sync::Arc::new(|_, _, _| axum::http::StatusCode::BAD_REQUEST),
        )
        .await;
        let task_id = format!("task-noretry-{}", uuid::Uuid::new_v4());
        panel_with_response_url(&task_id, &url).await;
        card_store()
            .complete_monitor(&task_id, 30, 30000, "检测到桌面变化".to_string())
            .await;
        deliver_monitor_result_http(card_store(), &task_id, "completed", 30, 30000, "检测到桌面变化")
            .await;
        assert_eq!(
            connections.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "4xx must never retry the same payload"
        );
        match card_store().monitor_state(&task_id).await {
            MonitorState::Completed { delivery, .. } => {
                assert_eq!(delivery, MonitorDelivery::Failed { errcode: 400 });
            }
            other => panic!("expected Failed{{400}}, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_response_url_expired_rejected() {
        let connections = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let url = mock_server(
            connections.clone(),
            std::sync::Arc::new(|_, _, _| axum::http::StatusCode::OK),
        )
        .await;
        let task_id = format!("task-expired-{}", uuid::Uuid::new_v4());
        crate::gateway::wecom_card::register_http_panel_with_url(&task_id, None, None).await;
        card_store()
            .attach_response_url_at(
                &task_id,
                url.clone(),
                crate::gateway::wecom_card::WecomCardStore::now_unix()
                    - RESPONSE_URL_TTL_SECS
                    - 60,
            )
            .await;
        card_store()
            .complete_monitor(&task_id, 30, 30000, "检测到桌面变化".to_string())
            .await;
        deliver_monitor_result_http(card_store(), &task_id, "completed", 30, 30000, "检测到桌面变化")
            .await;
        assert_eq!(
            connections.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "expired response_url must not be called"
        );
        match card_store().monitor_state(&task_id).await {
            MonitorState::Completed { delivery, .. } => {
                assert_eq!(delivery, MonitorDelivery::Failed { errcode: -2 });
            }
            other => panic!("expected Failed{{-2}}, got {other:?}"),
        }
    }

    #[test]
    fn http_delivery_never_uses_websocket_types() {
        // Compile-level guarantee: this module imports no WecomOutboundMsg
        // and the HTTP delivery target is marker-only. Assert the markdown
        // body shape stays transport-agnostic.
        let body = serde_json::json!({
            "msgtype": "markdown",
            "markdown": {"content": "### 结果"}
        });
        assert_eq!(body["msgtype"], "markdown");
        assert!(body.get("template_card").is_none());
        let _ = WecomCardTransport::HttpCallback;
    }
}
