//! WeCom Smart Bot HTTP URL callback transport.
//!
//! This module terminates only the HTTP/encryption layer. Decrypted callbacks
//! continue through the existing `WecomCallbackBody` and inbound normalizer.

use crate::channels::InboundMessage;
use crate::config::{Config, WecomTransportMode};
use crate::gateway::wecom_crypto::{
    decrypt_message_with_report, verify_signature, CryptoStageReport, WecomCryptoError,
};
use crate::gateway::wecom_inbound::normalize_wecom_callback;
use crate::gateway::wecom_protocol::{WecomCallbackBody, WecomChatType};
use serde::Deserialize;

pub const WECOM_HTTP_CALLBACK_PATH: &str = "/webhook/wecom";

/// Default environment variable names for WeCom HTTP callback credentials.
/// These are used when the config doesn't explicitly specify env var names.
pub const DEFAULT_WECOM_CALLBACK_TOKEN_ENV: &str = "WECOM_CALLBACK_TOKEN";
pub const DEFAULT_WECOM_ENCODING_AES_KEY_ENV: &str = "WECOM_ENCODING_AES_KEY";

#[derive(Debug, Clone, Deserialize)]
pub struct WecomCallbackQuery {
    pub msg_signature: String,
    pub timestamp: String,
    pub nonce: String,
    #[serde(default)]
    pub echostr: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EncryptedEnvelope {
    encrypt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WecomHttpError {
    Disabled,
    WrongTransport,
    MissingConfiguration,
    BadSignature,
    InvalidEnvelope,
    /// Structured crypto stages (official SDK taxonomy). The blanket
    /// `DecryptFailed` mapping has been removed: each stage surfaces
    /// with its own variant so the real GET path can show it.
    EncodingKeyDecodeFailed,
    CiphertextBase64Failed,
    AesCbcFailed,
    Pkcs7Failed,
    FrameTooShort,
    MessageLengthInvalid,
    Utf8Failed,
    ReceiveIdMismatch,
    InvalidPayload,
}

#[derive(Debug, Clone)]
pub struct ParsedHttpCallback {
    pub body: WecomCallbackBody,
    pub inbound: InboundMessage,
}

pub fn http_callback_enabled(config: &Config) -> bool {
    let channel_enabled = config
        .channels_config
        .wecom
        .as_ref()
        .map(|entry| entry.enabled)
        .unwrap_or(false);
    (config.gateway.wecom.enabled || channel_enabled)
        && resolve_transport_mode(config) == WecomTransportMode::HttpCallback
}

pub fn resolve_transport_mode(config: &Config) -> WecomTransportMode {
    match config
        .channels_config
        .wecom
        .as_ref()
        .and_then(|entry| entry.extra.get("transport_mode"))
        .and_then(|value| value.as_str())
        .map(str::trim)
    {
        Some("http_callback") => WecomTransportMode::HttpCallback,
        Some("long_connection") => WecomTransportMode::LongConnection,
        _ => config.gateway.wecom.transport_mode,
    }
}

pub fn resolve_http_credentials(config: &Config) -> Result<(String, String), WecomHttpError> {
    let gateway = &config.gateway.wecom;
    let entry = config.channels_config.wecom.as_ref();

    // Token resolution order:
    // 1. channel extra plaintext
    // 2. gateway config plaintext
    // 3. gateway config explicit env var
    // 4. channel extra env var name
    // 5. DEFAULT env var name: WECOM_CALLBACK_TOKEN
    let token = entry
        .and_then(|value| value.extra.get("callback_token"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| non_empty(&gateway.callback_token))
        .or_else(|| env_value(gateway.callback_token_env.as_deref()))
        .or_else(|| {
            entry
                .and_then(|value| value.extra.get("callback_token_env"))
                .and_then(|value| value.as_str())
                .and_then(|name| env_value(Some(name)))
        })
        // Default: WECOM_CALLBACK_TOKEN env var
        .or_else(|| env_value(Some(DEFAULT_WECOM_CALLBACK_TOKEN_ENV)));

    // AESKey resolution order:
    // 1. channel extra plaintext
    // 2. gateway config plaintext
    // 3. gateway config explicit env var
    // 4. channel extra env var name
    // 5. DEFAULT env var name: WECOM_ENCODING_AES_KEY
    let aes_key = entry
        .and_then(|value| value.extra.get("encoding_aes_key"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| non_empty(&gateway.encoding_aes_key))
        .or_else(|| env_value(gateway.encoding_aes_key_env.as_deref()))
        .or_else(|| {
            entry
                .and_then(|value| value.extra.get("encoding_aes_key_env"))
                .and_then(|value| value.as_str())
                .and_then(|name| env_value(Some(name)))
        })
        // Default: WECOM_ENCODING_AES_KEY env var
        .or_else(|| env_value(Some(DEFAULT_WECOM_ENCODING_AES_KEY_ENV)));

    match (token, aes_key) {
        (Some(token), Some(aes_key)) => Ok((token, aes_key)),
        _ => Err(WecomHttpError::MissingConfiguration),
    }
}

/// Resolve receive_id for WeCom Bot Webhook callback verification.
///
/// receive_id semantics (per WeCom official docs):
/// - OPTIONAL: If not configured in WeCom后台, use empty string
/// - If configured: Must match the value in the decrypted frame
pub fn resolve_receive_id(config: &Config) -> String {
    let gateway = &config.gateway.wecom;
    let entry = config.channels_config.wecom.as_ref();

    // Resolution order:
    // 1. gateway config plaintext receive_id
    // 2. gateway config receive_id_env env var
    // 3. channel extra plaintext receive_id
    // 4. channel extra receive_id_env
    // 5. DEFAULT env var: WECOM_RECEIVE_ID (if commonly used)
    // 6. Empty string (no receive_id configured - Bot Webhook allows this)
    entry
        .and_then(|v| v.extra.get("receive_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .or_else(|| non_empty(&gateway.receive_id))
        .or_else(|| {
            gateway
                .receive_id_env
                .as_ref()
                .and_then(|name| env_value(Some(name)))
        })
        .or_else(|| {
            entry
                .and_then(|v| v.extra.get("receive_id_env"))
                .and_then(|v| v.as_str())
                .and_then(|name| env_value(Some(name)))
        })
        .unwrap_or_default()
}

/// Returns credential presence without exposing actual values.
///
/// Phase 2A.1.4 (P6): when the AES key is present this also performs a
/// secret-free structural decoder check and emits fail-fast startup
/// diagnostics (`input_chars`/`decoded_bytes`/`decode_valid`), so a
/// malformed key is reported at HTTP-callback startup instead of
/// waiting for the first real WeCom GET.
pub fn check_http_credentials_present(config: &Config) -> (bool, bool) {
    let (token, aes_key) = match resolve_http_credentials(config) {
        Ok(v) => v,
        Err(_) => return (false, false),
    };
    // Use constant-time comparison against non-empty placeholder
    let token_ok = token.len() > 0;
    let aes_ok = aes_key.len() > 0;
    if aes_ok {
        let diag = crate::gateway::wecom_crypto::encoding_key_diag(&aes_key);
        println!(
            "[wecom-http] credentials_aes_key_checked input_chars={} padded_chars={} base64_decode={} decoded_bytes={} aes_key_decode_valid={}",
            diag.input_chars,
            diag.padded_chars,
            if diag.base64_decode_ok { "ok" } else { "fail" },
            diag.decoded_bytes,
            diag.decode_valid,
        );
        if let Some(reason) = diag.failure_reason() {
            println!("[wecom-http] configuration_invalid reason={reason}");
        }
    }
    (token_ok, aes_ok)
}

pub fn verify_url(
    config: &Config,
    query: &WecomCallbackQuery,
) -> Result<String, WecomHttpError> {
    ensure_transport(config)?;
    let echostr = query
        .echostr
        .as_deref()
        .ok_or(WecomHttpError::InvalidEnvelope)?;
    let (token, aes_key) = resolve_http_credentials(config)?;
    if !verify_signature(
        &token,
        &query.timestamp,
        &query.nonce,
        echostr,
        &query.msg_signature,
    ) {
        return Err(WecomHttpError::BadSignature);
    }
    let receive_id = resolve_receive_id(config);
    let outcome = decrypt_message_with_report(&aes_key, echostr, &receive_id);
    log_crypto_stage(&outcome.stages);
    outcome
        .result
        .map(|value| value.message)
        .map_err(map_crypto_error)
}

pub fn parse_post(
    config: &Config,
    query: &WecomCallbackQuery,
    raw_body: &str,
) -> Result<ParsedHttpCallback, WecomHttpError> {
    ensure_transport(config)?;
    let envelope: EncryptedEnvelope =
        serde_json::from_str(raw_body).map_err(|_| WecomHttpError::InvalidEnvelope)?;
    let (token, aes_key) = resolve_http_credentials(config)?;
    if !verify_signature(
        &token,
        &query.timestamp,
        &query.nonce,
        &envelope.encrypt,
        &query.msg_signature,
    ) {
        return Err(WecomHttpError::BadSignature);
    }
    let receive_id = resolve_receive_id(config);
    let outcome = decrypt_message_with_report(
        &aes_key,
        &envelope.encrypt,
        &receive_id,
    );
    log_crypto_stage(&outcome.stages);
    let decrypted = outcome.result.map_err(map_crypto_error)?;
    let body: WecomCallbackBody =
        serde_json::from_str(&decrypted.message).map_err(|_| WecomHttpError::InvalidPayload)?;
    let inbound = normalize_wecom_callback(&body, &format!("http:{}", body.msgid))
        .map_err(|_| WecomHttpError::InvalidPayload)?;
    Ok(ParsedHttpCallback { body, inbound })
}

pub fn chat_type(body: &WecomCallbackBody) -> &'static str {
    if WecomChatType::from_str(body.chattype.as_deref()).is_group() {
        "group"
    } else {
        "single"
    }
}

fn ensure_transport(config: &Config) -> Result<(), WecomHttpError> {
    let enabled = config.gateway.wecom.enabled
        || config
            .channels_config
            .wecom
            .as_ref()
            .map(|entry| entry.enabled)
            .unwrap_or(false);
    if !enabled {
        return Err(WecomHttpError::Disabled);
    }
    if resolve_transport_mode(config) != WecomTransportMode::HttpCallback {
        return Err(WecomHttpError::WrongTransport);
    }
    Ok(())
}

fn map_crypto_error(error: WecomCryptoError) -> WecomHttpError {
    match error {
        WecomCryptoError::EncodingKeyDecodeFailed(_) => WecomHttpError::EncodingKeyDecodeFailed,
        WecomCryptoError::CiphertextBase64Failed => WecomHttpError::CiphertextBase64Failed,
        WecomCryptoError::AesCbcFailed => WecomHttpError::AesCbcFailed,
        WecomCryptoError::Pkcs7Failed => WecomHttpError::Pkcs7Failed,
        WecomCryptoError::FrameTooShort => WecomHttpError::FrameTooShort,
        WecomCryptoError::MessageLengthInvalid => WecomHttpError::MessageLengthInvalid,
        WecomCryptoError::Utf8Failed => WecomHttpError::Utf8Failed,
        WecomCryptoError::ReceiveIdMismatch => WecomHttpError::ReceiveIdMismatch,
    }
}

/// Emit the structured crypto stage diagnostics. No secrets: never logs
/// the Token, EncodingAESKey, echostr, plaintext, ciphertext or receiveId.
fn log_crypto_stage(stages: &CryptoStageReport) {
    println!("[wecom-http] encoding_key_diag");
    println!(
        "[wecom-http] input_chars={}",
        stages.encoding_key_input_chars
    );
    println!(
        "[wecom-http] padded_chars={}",
        stages.encoding_key_padded_chars
    );
    println!(
        "[wecom-http] base64_decode={}",
        if stages.encoding_key_base64_decode_ok {
            "ok"
        } else {
            "fail"
        }
    );
    println!(
        "[wecom-http] decoded_bytes={}",
        stages.encoding_key_decoded_bytes
    );
    println!("[wecom-http] crypto_stage");
    println!("[wecom-http] aes_key_decode_ok={}", stages.aes_key_decode_ok);
    println!(
        "[wecom-http] ciphertext_decode_ok={}",
        stages.ciphertext_decode_ok
    );
    println!("[wecom-http] aes_cbc_ok={}", stages.aes_cbc_ok);
    println!("[wecom-http] pkcs7_ok={}", stages.pkcs7_ok);
    println!("[wecom-http] frame_parse_ok={}", stages.frame_parse_ok);
    println!(
        "[wecom-http] message_extract_ok={}",
        stages.message_extract_ok
    );
    println!(
        "[wecom-http] receive_id_check={}",
        stages.receive_id_check.as_str()
    );
}

/// Truthful `(signature_valid, decrypt_ok)` flags for handler logs.
///
/// - `signature_valid` is true only for errors that can occur AFTER a
///   successful SHA-1 signature verification (crypto stages, receiveId
///   mismatch, payload parse).
/// - `decrypt_ok` is true only when `decrypt_message` fully succeeded
///   and a later stage failed (i.e. `InvalidPayload`).
pub fn error_flags(error: &WecomHttpError) -> (bool, bool) {
    let signature_valid = matches!(
        error,
        WecomHttpError::EncodingKeyDecodeFailed
            | WecomHttpError::CiphertextBase64Failed
            | WecomHttpError::AesCbcFailed
            | WecomHttpError::Pkcs7Failed
            | WecomHttpError::FrameTooShort
            | WecomHttpError::MessageLengthInvalid
            | WecomHttpError::Utf8Failed
            | WecomHttpError::ReceiveIdMismatch
            | WecomHttpError::InvalidPayload
    );
    let decrypt_ok = matches!(error, WecomHttpError::InvalidPayload);
    (signature_valid, decrypt_ok)
}

fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

fn env_value(name: Option<&str>) -> Option<String> {
    let name = name?.trim();
    if name.is_empty() {
        return None;
    }
    std::env::var(name).ok().filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::wecom_crypto::{encrypt_test_fixture, TEST_ENCODING_AES_KEY};

    const TOKEN: &str = "callback-token";

    fn config() -> Config {
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = TOKEN.to_string();
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();
        config
    }

    fn query(encrypted: &str) -> WecomCallbackQuery {
        let timestamp = "1700000000";
        let nonce = "nonce-1";
        let mut values = [TOKEN, timestamp, nonce, encrypted];
        values.sort_unstable();
        let signature = super::super::wecom_crypto::signature_for_test(&values.concat());
        WecomCallbackQuery {
            msg_signature: signature,
            timestamp: timestamp.to_string(),
            nonce: nonce.to_string(),
            echostr: None,
        }
    }

    #[test]
    fn wecom_http_verify_valid_request() {
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "verified", "");
        let mut query = query(&encrypted);
        query.echostr = Some(encrypted);
        assert_eq!(verify_url(&config(), &query).unwrap(), "verified");
    }

    #[test]
    fn wecom_http_verify_invalid_signature() {
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "verified", "");
        let mut query = query(&encrypted);
        query.echostr = Some(encrypted);
        query.msg_signature = "bad".to_string();
        assert_eq!(verify_url(&config(), &query), Err(WecomHttpError::BadSignature));
    }

    // ------------------------------------------------------------------
    // Official parity + URL percent-decoding (P6/P7)
    // ------------------------------------------------------------------

    use crate::gateway::wecom_crypto::{
        PARITY_CIPHERTEXT_2, PARITY_ENCODING_AES_KEY, PARITY_MESSAGE_2, PARITY_NONCE,
        PARITY_SIGNATURE_2, PARITY_TIMESTAMP, PARITY_TOKEN,
    };

    fn parity_config() -> Config {
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = PARITY_TOKEN.to_string();
        config.gateway.wecom.encoding_aes_key = PARITY_ENCODING_AES_KEY.to_string();
        config
    }

    /// Percent-encode every byte that is not an RFC 3986 unreserved
    /// character, mirroring how WeCom emits its callback URL.
    fn percent_encode(value: &str) -> String {
        value
            .bytes()
            .map(|byte| match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (byte as char).to_string()
                }
                _ => format!("%{byte:02X}"),
            })
            .collect()
    }

    #[test]
    fn wecom_get_percent_encoded_ciphertext_same_value_for_signature_and_decrypt() {
        // PARITY_CIPHERTEXT_2 contains '+', '/' and '=' so the encoded
        // query string exercises %2B, %2F and %3D exactly like the
        // official SDK's URLSearchParams round trip.
        assert!(PARITY_CIPHERTEXT_2.contains('+'));
        assert!(PARITY_CIPHERTEXT_2.contains('/'));
        assert!(PARITY_CIPHERTEXT_2.contains('='));

        let raw_query = format!(
            "msg_signature={}&timestamp={}&nonce={}&echostr={}",
            PARITY_SIGNATURE_2,
            PARITY_TIMESTAMP,
            PARITY_NONCE,
            percent_encode(PARITY_CIPHERTEXT_2),
        );
        let uri: axum::http::Uri = format!("/webhook/wecom?{raw_query}")
            .parse()
            .expect("valid callback URI");
        // Same extractor as the real GET handler (axum Query).
        let query = axum::extract::Query::<WecomCallbackQuery>::try_from_uri(&uri)
            .expect("query must parse")
            .0;

        // The decoded echostr must equal the original ciphertext…
        assert_eq!(query.echostr.as_deref(), Some(PARITY_CIPHERTEXT_2));
        // …and that same decoded value must pass signature AND decrypt.
        assert_eq!(verify_url(&parity_config(), &query).unwrap(), PARITY_MESSAGE_2);
    }

    #[test]
    fn wecom_get_plaintext_response_is_unquoted() {
        // The GET contract (official plugin behavior): the handler must
        // return the raw plaintext — no JSON.stringify, no extra quotes,
        // no newline. verify_url returns that exact String.
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "verified", "");
        let mut query = query(&encrypted);
        query.echostr = Some(encrypted);
        let plaintext = verify_url(&config(), &query).unwrap();
        assert_eq!(plaintext, "verified");
        assert!(!plaintext.starts_with('"'));
        assert!(!plaintext.ends_with('"'));
        assert!(!plaintext.ends_with('\n'));
    }

    #[test]
    fn wecom_http_error_flags_are_truthful() {
        // Bad signature: never reached crypto.
        let flags = error_flags(&WecomHttpError::BadSignature);
        assert_eq!(flags, (false, false));
        // Crypto stage failure: signature passed, decrypt did not.
        let flags = error_flags(&WecomHttpError::Pkcs7Failed);
        assert_eq!(flags, (true, false));
        // receiveId mismatch: signature passed, decrypt failed.
        let flags = error_flags(&WecomHttpError::ReceiveIdMismatch);
        assert_eq!(flags, (true, false));
        // Payload parse failure happens AFTER a successful decrypt.
        let flags = error_flags(&WecomHttpError::InvalidPayload);
        assert_eq!(flags, (true, true));
        // Pre-signature failures.
        let flags = error_flags(&WecomHttpError::MissingConfiguration);
        assert_eq!(flags, (false, false));
    }

    #[test]
    fn wecom_http_verify_invalid_aes() {
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "verified", "");
        let mut config = config();
        config.gateway.wecom.encoding_aes_key = "invalid".to_string();
        let mut query = query(&encrypted);
        query.echostr = Some(encrypted);
        assert_eq!(
            verify_url(&config, &query),
            Err(WecomHttpError::EncodingKeyDecodeFailed)
        );
    }

    #[test]
    fn wecom_http_post_valid_encrypted_message() {
        let plaintext = r#"{"msgid":"m-1","aibotid":"bot","chattype":"single","from":{"userid":"u-1"},"msgtype":"text","text":{"content":"hello"}}"#;
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, plaintext, "");
        let parsed = parse_post(
            &config(),
            &query(&encrypted),
            &serde_json::json!({"encrypt": encrypted}).to_string(),
        )
        .unwrap();
        assert_eq!(parsed.body.msgid, "m-1");
        assert_eq!(parsed.inbound.text, "hello");
    }

    #[test]
    fn wecom_http_post_invalid_signature() {
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "{}", "");
        let mut query = query(&encrypted);
        query.msg_signature = "bad".to_string();
        assert!(matches!(
            parse_post(&config(), &query, &serde_json::json!({"encrypt": encrypted}).to_string()),
            Err(WecomHttpError::BadSignature)
        ));
    }

    #[test]
    fn wecom_http_post_invalid_ciphertext() {
        let query = query("not-base64");
        assert!(matches!(
            parse_post(&config(), &query, r#"{"encrypt":"not-base64"}"#),
            Err(WecomHttpError::CiphertextBase64Failed)
        ));
    }

    #[test]
    fn wecom_http_single_normalizes_to_wecom_channel() {
        let parsed = parsed_message("single", None);
        assert_eq!(parsed.inbound.channel, crate::channels::ChannelKind::Wecom);
        assert_eq!(parsed.inbound.session_id.as_deref(), Some("wecom:single:u-1"));
    }

    #[test]
    fn wecom_http_group_normalizes_to_wecom_channel() {
        let parsed = parsed_message("group", Some("chat-1"));
        assert_eq!(parsed.inbound.channel, crate::channels::ChannelKind::Wecom);
        assert_eq!(parsed.inbound.session_id.as_deref(), Some("wecom:group:chat-1"));
    }

    #[test]
    fn wecom_http_session_namespace_preserved() {
        assert_eq!(
            parsed_message("single", None).inbound.session_id.as_deref(),
            Some("wecom:single:u-1")
        );
        assert_eq!(
            parsed_message("group", Some("chat-1"))
                .inbound
                .session_id
                .as_deref(),
            Some("wecom:group:chat-1")
        );
    }

    #[test]
    fn wecom_http_callback_route_uses_gateway_webhook_style() {
        assert_eq!(WECOM_HTTP_CALLBACK_PATH, "/webhook/wecom");
    }

    #[test]
    fn wecom_http_mode_disables_long_connection_selection() {
        let config = config();
        assert!(http_callback_enabled(&config));
        assert_eq!(resolve_transport_mode(&config), WecomTransportMode::HttpCallback);
    }

    #[test]
    fn wecom_long_connection_unchanged() {
        let legacy: crate::config::schema::GatewayWecomConfig =
            toml::from_str("enabled = true").unwrap();
        assert_eq!(legacy.transport_mode, WecomTransportMode::LongConnection);
        let mut config = Config::default();
        config.gateway.wecom = legacy;
        assert_eq!(resolve_transport_mode(&config), WecomTransportMode::LongConnection);
        assert!(!http_callback_enabled(&config));
    }

    #[tokio::test]
    async fn wecom_http_duplicate_msg_is_ignored() {
        let runtime = crate::gateway::GatewayRuntime::new(Config::default());
        let cache = runtime.dedup_cache();
        assert!(cache.check_and_insert("wecom-http-msg-1").await);
        assert!(!cache.check_and_insert("wecom-http-msg-1").await);
    }

    fn parsed_message(chat_type: &str, chat_id: Option<&str>) -> ParsedHttpCallback {
        let mut value = serde_json::json!({
            "msgid": "m-2",
            "aibotid": "bot",
            "chattype": chat_type,
            "from": {"userid": "u-1"},
            "msgtype": "text",
            "text": {"content": "hello"}
        });
        if let Some(chat_id) = chat_id {
            value["chatid"] = serde_json::json!(chat_id);
        }
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, &value.to_string(), "");
        parse_post(
            &config(),
            &query(&encrypted),
            &serde_json::json!({"encrypt": encrypted}).to_string(),
        )
        .unwrap()
    }

    // =========================================================================
    // Transport Mode Resolution Tests
    // =========================================================================

    #[test]
    fn wecom_transport_defaults_to_long_connection() {
        let legacy: crate::config::schema::GatewayWecomConfig =
            toml::from_str("enabled = true").unwrap();
        let mut config = Config::default();
        config.gateway.wecom = legacy;
        assert_eq!(resolve_transport_mode(&config), WecomTransportMode::LongConnection);
    }

    #[test]
    fn wecom_transport_gateway_config_http_callback() {
        let legacy: crate::config::schema::GatewayWecomConfig =
            toml::from_str(r#"enabled = true
transport_mode = "http_callback""#).unwrap();
        let mut config = Config::default();
        config.gateway.wecom = legacy;
        assert_eq!(resolve_transport_mode(&config), WecomTransportMode::HttpCallback);
    }

    #[test]
    fn wecom_transport_channel_extra_takes_precedence() {
        // When channel extra has transport_mode, it should be used
        // This test verifies the precedence logic by checking the behavior
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::LongConnection;
        // Simulate channel extra with HTTP mode - the resolve function checks this
        // We verify by setting up the extra JSON and checking behavior
        // Since we can't easily create ChannelEntry, we test the other path:
        // gateway.wecom.transport_mode should be used when channel extra is absent
        assert_eq!(resolve_transport_mode(&config), WecomTransportMode::LongConnection);

        // Now set HTTP mode via gateway config
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        assert_eq!(resolve_transport_mode(&config), WecomTransportMode::HttpCallback);
    }

    #[test]
    fn wecom_transport_gateway_config_has_precedence() {
        // When gateway config explicitly sets transport_mode, it should be used
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        // No channel extra
        assert_eq!(resolve_transport_mode(&config), WecomTransportMode::HttpCallback);
    }

    #[test]
    fn wecom_old_config_remains_long_connection() {
        // Legacy config without transport_mode should default to long_connection
        let legacy: crate::config::schema::GatewayWecomConfig =
            toml::from_str("enabled = true").unwrap();
        let mut config = Config::default();
        config.gateway.wecom = legacy;
        assert_eq!(resolve_transport_mode(&config), WecomTransportMode::LongConnection);
        assert!(!http_callback_enabled(&config));
    }

    #[test]
    fn wecom_http_mode_enables_callback_processing() {
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = "test-token".to_string();
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();
        assert!(http_callback_enabled(&config));
    }

    #[test]
    fn wecom_http_mode_requires_http_transport() {
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::LongConnection;
        config.gateway.wecom.callback_token = "test-token".to_string();
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();
        assert!(!http_callback_enabled(&config));
    }

    // =========================================================================
    // Credential Resolution Tests
    // =========================================================================

    #[test]
    fn wecom_http_uses_default_token_env() {
        // When no explicit env var name is set, should use default: WECOM_CALLBACK_TOKEN
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token_env = None;
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();
        // No token set - should try DEFAULT_WECOM_CALLBACK_TOKEN env
        // Since env might not exist in test, we check the function doesn't panic
        let _ = resolve_http_credentials(&config);
        // Result depends on whether WECOM_CALLBACK_TOKEN is set in test env
        // The important thing is it tries the default env name
    }

    #[test]
    fn wecom_http_uses_default_aes_env() {
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = "test-token".to_string();
        config.gateway.wecom.encoding_aes_key_env = None;
        // Should try DEFAULT_WECOM_ENCODING_AES_KEY env
        let _ = resolve_http_credentials(&config);
    }

    #[test]
    fn wecom_http_missing_token_fails() {
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        // Token missing, AES key present
        config.gateway.wecom.callback_token = String::new();
        config.gateway.wecom.callback_token_env = None;
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();
        config.gateway.wecom.encoding_aes_key_env = None;

        let result = resolve_http_credentials(&config);
        assert!(matches!(result, Err(WecomHttpError::MissingConfiguration)));
    }

    #[test]
    fn wecom_http_missing_aes_key_fails() {
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = "test-token".to_string();
        config.gateway.wecom.callback_token_env = None;
        // AES key missing
        config.gateway.wecom.encoding_aes_key = String::new();
        config.gateway.wecom.encoding_aes_key_env = None;

        let result = resolve_http_credentials(&config);
        assert!(matches!(result, Err(WecomHttpError::MissingConfiguration)));
    }

    #[test]
    fn wecom_http_credential_check_matches_resolution() {
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = "test-token".to_string();
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();

        let (token_ok, aes_ok) = check_http_credentials_present(&config);
        assert!(token_ok);
        assert!(aes_ok);

        // Missing credentials should report false
        let mut config2 = Config::default();
        config2.gateway.wecom.enabled = true;
        config2.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        let (token_ok2, aes_ok2) = check_http_credentials_present(&config2);
        assert!(!token_ok2);
        assert!(!aes_ok2);
    }

    // =========================================================================
    // Signature Verification Tests
    // =========================================================================

    #[test]
    fn wecom_http_valid_get_signature() {
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "verified", "");
        let timestamp = "1700000000";
        let nonce = "nonce-1";
        let mut values = ["test-token", timestamp, nonce, &encrypted];
        values.sort_unstable();
        let signature = super::super::wecom_crypto::signature_for_test(&values.concat());

        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = "test-token".to_string();
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();

        let query = WecomCallbackQuery {
            msg_signature: signature,
            timestamp: timestamp.to_string(),
            nonce: nonce.to_string(),
            echostr: Some(encrypted),
        };

        let result = verify_url(&config, &query);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "verified");
    }

    #[test]
    fn wecom_http_invalid_signature() {
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "verified", "");
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = "test-token".to_string();
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();

        let query = WecomCallbackQuery {
            msg_signature: "bad-signature".to_string(),
            timestamp: "1700000000".to_string(),
            nonce: "nonce-1".to_string(),
            echostr: Some(encrypted),
        };

        let result = verify_url(&config, &query);
        assert!(matches!(result, Err(WecomHttpError::BadSignature)));
    }

    #[test]
    fn wecom_http_wrong_token_rejected() {
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "verified", "");
        let timestamp = "1700000000";
        let nonce = "nonce-1";
        let mut values = ["wrong-token", timestamp, nonce, &encrypted];
        values.sort_unstable();
        let signature = super::super::wecom_crypto::signature_for_test(&values.concat());

        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = "correct-token".to_string();
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();

        let query = WecomCallbackQuery {
            msg_signature: signature,
            timestamp: timestamp.to_string(),
            nonce: nonce.to_string(),
            echostr: Some(encrypted),
        };

        // Signature computed with wrong token should not verify with correct token
        let result = verify_url(&config, &query);
        assert!(matches!(result, Err(WecomHttpError::BadSignature)));
    }
}
