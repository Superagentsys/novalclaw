//! Shared HTTP client plumbing for the non-browser web tools
//! (`web_fetch`, `http_request`, `web_search`).
//!
//! One builder owns proxy wiring, timeouts, redirect policy, and the
//! private-network (SSRF) guard so the three tools cannot drift apart again.
//! The browser tool deliberately keeps its Chromium network stack and never
//! goes through this module.

use crate::config::schema::ProxyConfig;
use reqwest::redirect::Attempt;
use reqwest::{Client, NoProxy, Proxy};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;
use url::{Host, Url};

/// Identifies OmniNova on outbound web-tool requests.
pub const WEB_USER_AGENT: &str = concat!("OmniNova/", env!("CARGO_PKG_VERSION"));

/// Unified connect timeout. Not configurable yet; one constant keeps the three
/// tools identical.
pub const DEFAULT_WEB_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Unified default overall request timeout. Tools override it from their own
/// `[web_fetch]` / `[http_request]` / `[web_search]` `timeout_secs` settings.
pub const DEFAULT_WEB_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Hard ceiling for the configured request timeout.
pub const MAX_WEB_REQUEST_TIMEOUT_SECS: u64 = 600;

/// Unified redirect limit for every web tool.
pub const WEB_MAX_REDIRECTS: usize = 10;

/// True when `host` is `domain` or a subdomain of `domain` (dot-delimited).
/// `evil-example.com` does **not** match `example.com`.
pub fn host_matches_allowlist(host: &str, allowed: &[String]) -> bool {
    if allowed.is_empty() || allowed.iter().any(|entry| entry.trim() == "*") {
        return true;
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    allowed.iter().any(|entry| {
        let domain = entry.trim_end_matches('.').to_ascii_lowercase();
        !domain.is_empty() && (host == domain || host.ends_with(&format!(".{domain}")))
    })
}

/// Strip URL userinfo so errors and logs never echo `user:password@host`.
pub fn redact_url(url: &Url) -> String {
    if url.username().is_empty() && url.password().is_none() {
        return url.as_str().to_string();
    }
    let mut redacted = url.clone();
    let _ = redacted.set_username("***");
    let _ = redacted.set_password(Some("***"));
    redacted.to_string()
}

/// Redact userinfo in a string that may be a URL or contain URL-like spans.
pub fn redact_secrets_in_text(text: &str) -> String {
    if let Ok(url) = Url::parse(text.trim()) {
        return redact_url(&url);
    }
    redact_embedded_userinfo(text)
}

fn redact_embedded_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(scheme_end) = find_scheme_slash_slash(bytes, i) {
            out.push_str(&text[i..scheme_end]);
            i = scheme_end;
            if let Some(at) = bytes[i..].iter().position(|&b| b == b'@') {
                let userinfo = &bytes[i..i + at];
                if userinfo.contains(&b':')
                    && !userinfo
                        .iter()
                        .any(|b| *b == b'/' || *b == b' ' || *b == b'\n')
                {
                    out.push_str("***:***");
                    i += at;
                    continue;
                }
            }
        }
        out.push(text[i..].chars().next().unwrap_or('?'));
        i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    out
}

fn find_scheme_slash_slash(bytes: &[u8], from: usize) -> Option<usize> {
    let window = bytes.get(from..)?;
    let pos = window.windows(3).position(|w| w == b"://")?;
    Some(from + pos + 3)
}

/// Unified default response size boundary.
pub const DEFAULT_WEB_MAX_RESPONSE_BYTES: usize = 1_048_576;

/// Hard ceiling on request bodies sent by `http_request`.
pub const MAX_REQUEST_BODY_BYTES: usize = 1_048_576;

// ---------------------------------------------------------------------------
// Structured network errors
// ---------------------------------------------------------------------------

/// Minimal structured classification for web-tool network failures. The
/// `Display` text is what agents see, so every variant carries a recognizable
/// `[Category]` prefix.
#[derive(Debug, thiserror::Error)]
pub enum WebClientError {
    #[error("[InvalidUrl] {detail}")]
    InvalidUrl { detail: String },
    #[error("[ProxyConfigurationInvalid] {detail}")]
    ProxyConfigurationInvalid { detail: String },
    #[error("[ProxyConnectionFailed] {detail}")]
    ProxyConnectionFailed { detail: String },
    #[error("[DnsFailure] host={host}: {detail}")]
    DnsFailure { host: String, detail: String },
    #[error("[TlsFailure] {detail}")]
    TlsFailure { detail: String },
    #[error("[ConnectionFailed] {detail}")]
    ConnectionFailed { detail: String },
    #[error("[ConnectionTimeout] {detail}")]
    ConnectionTimeout { detail: String },
    #[error("[RequestTimeout] {detail}")]
    RequestTimeout { detail: String },
    #[error("[RedirectError] {detail}")]
    RedirectError { detail: String },
    #[error("[PrivateNetworkBlocked] host={host}: {detail}")]
    PrivateNetworkBlocked { host: String, detail: String },
    #[error("[HttpStatusError] HTTP {status}")]
    HttpStatusError { status: u16 },
    #[error("[RequestTooLarge] {detail}")]
    RequestTooLarge { detail: String },
    #[error("[ResponseTooLarge] {detail}")]
    ResponseTooLarge { detail: String },
    #[error("[HeaderRejected] {detail}")]
    HeaderRejected { detail: String },
    #[error("[BodyReadFailed] {detail}")]
    BodyReadFailed { detail: String },
}

/// Maps a reqwest error into the structured taxonomy. reqwest does not expose
/// typed connect-phase causes across versions, so the classification inspects
/// the error and its source chain with stable predicates plus a text scan of
/// the source chain as a fallback.
pub fn map_reqwest_error(err: reqwest::Error) -> WebClientError {
    let text = redact_secrets_in_text(&err.to_string());
    let source_text = redact_secrets_in_text(&source_chain_text(&err));

    if err.is_redirect() {
        if text.contains("PrivateNetworkBlocked")
            || text.contains("private network")
            || source_text.contains("PrivateNetworkBlocked")
            || source_text.contains("private network")
        {
            return WebClientError::PrivateNetworkBlocked {
                host: String::new(),
                detail: text,
            };
        }
        return WebClientError::RedirectError { detail: text };
    }
    if err.is_timeout() {
        return if err.is_connect() {
            WebClientError::ConnectionTimeout { detail: text }
        } else {
            WebClientError::RequestTimeout { detail: text }
        };
    }
    if err.is_connect() {
        let combined = format!("{text} {source_text}");
        if combined.contains("proxy") {
            return WebClientError::ProxyConnectionFailed { detail: text };
        }
        if combined.contains("failed to lookup")
            || combined.contains("dns")
            || combined.contains("DNS")
            || combined.contains("resolve")
            || combined.contains("no address")
        {
            return WebClientError::DnsFailure {
                host: String::new(),
                detail: text,
            };
        }
        if combined.contains("certificate")
            || combined.contains("tls")
            || combined.contains("ssl")
            || combined.contains("handshake")
        {
            return WebClientError::TlsFailure { detail: text };
        }
        return WebClientError::ConnectionFailed { detail: text };
    }
    if err.is_body() || err.is_decode() || err.is_request() {
        return WebClientError::BodyReadFailed { detail: text };
    }
    WebClientError::ConnectionFailed { detail: text }
}

fn source_chain_text(err: &reqwest::Error) -> String {
    let mut chain = String::new();
    let mut source: Option<&dyn std::error::Error> = std::error::Error::source(err);
    let mut depth = 0;
    while let Some(current) = source {
        if depth >= 6 {
            break;
        }
        chain.push(' ');
        chain.push_str(&current.to_string());
        source = current.source();
        depth += 1;
    }
    chain
}

// ---------------------------------------------------------------------------
// Proxy settings
// ---------------------------------------------------------------------------

/// Proxy settings resolved from `[proxy]` in config (environment variables are
/// already merged into that struct by `config::env`). `enabled` is an explicit
/// opt-in: proxy URLs that are merely present but not enabled are ignored, so
/// the disabled state is deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxySettings {
    pub enabled: bool,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub all_proxy: Option<String>,
    pub no_proxy: Option<String>,
}

impl ProxySettings {
    pub fn from_config(cfg: &ProxyConfig) -> Self {
        fn clean(value: &Option<String>) -> Option<String> {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        }
        Self {
            enabled: cfg.enabled,
            http_proxy: clean(&cfg.http_proxy),
            https_proxy: clean(&cfg.https_proxy),
            all_proxy: clean(&cfg.all_proxy),
            no_proxy: clean(&cfg.no_proxy),
        }
    }

    /// True when a proxy should actually be applied to requests.
    pub fn is_active(&self) -> bool {
        self.enabled
            && (self.http_proxy.is_some() || self.https_proxy.is_some() || self.all_proxy.is_some())
    }

    /// Effective proxy URL for a target scheme. `https` targets use
    /// `https_proxy` or `all_proxy`; `http` targets use `http_proxy` or
    /// `all_proxy`. No cross fallback between http and https entries.
    pub fn for_scheme(&self, scheme: &str) -> Option<&str> {
        if !self.enabled {
            return None;
        }
        match scheme {
            "https" => self.https_proxy.as_deref().or(self.all_proxy.as_deref()),
            "http" => self.http_proxy.as_deref().or(self.all_proxy.as_deref()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Private-network (SSRF) policy
// ---------------------------------------------------------------------------

fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        // Shared address space (RFC 6598, includes cloud metadata in some
        // environments); not stable in std, so matched manually.
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        // Benchmarking range (RFC 2544), never a legitimate web target.
        || (o[0] == 198 && (18..=19).contains(&o[1]))
}

fn is_private_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return true;
    }
    let segments = ip.segments();
    // Unique local (fc00::/7) and link-local (fe80::/10).
    if (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_private_ipv4(v4);
    }
    // Deprecated IPv4-compatible form ::a.b.c.d — block conservatively.
    if segments[0..5].iter().all(|s| *s == 0) && segments[5] != 0 {
        return true;
    }
    false
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_ipv4(v4),
        IpAddr::V6(v6) => is_private_ipv6(v6),
    }
}

fn private_blocked(host: String) -> WebClientError {
    WebClientError::PrivateNetworkBlocked {
        host,
        detail: "destination is a private, loopback, link-local, or otherwise non-public address"
            .to_string(),
    }
}

/// Whether a private host may be reached because the user explicitly listed it
/// in the tool's `allowed_domains`. This is the existing explicit opt-in
/// mechanism; no new permission surface is introduced.
fn host_is_explicitly_allowed(host_forms: &[String], allowed: &[String]) -> bool {
    allowed
        .iter()
        .any(|a| a.trim() == "*" || host_forms.iter().any(|f| f == a))
}

fn check_host_literal(host: Host<&str>, allowed: &[String]) -> Result<(), WebClientError> {
    match host {
        Host::Ipv4(ip) => {
            let display = ip.to_string();
            if is_private_ipv4(ip) && !host_is_explicitly_allowed(&[display.clone()], allowed) {
                return Err(private_blocked(display));
            }
            Ok(())
        }
        Host::Ipv6(ip) => {
            let bare = ip.to_string();
            let bracketed = format!("[{bare}]");
            if is_private_ipv6(ip)
                && !host_is_explicitly_allowed(&[bare.clone(), bracketed], allowed)
            {
                return Err(private_blocked(bare));
            }
            Ok(())
        }
        Host::Domain(domain) => {
            let lowered = domain.to_lowercase();
            let is_local = lowered == "localhost" || lowered.ends_with(".localhost");
            if is_local && !host_is_explicitly_allowed(&[lowered.clone()], allowed) {
                return Err(private_blocked(lowered));
            }
            Ok(())
        }
    }
}

/// Synchronous destination check without DNS resolution. Used inside the
/// redirect policy (which cannot await) and as the fast path for IP-literal
/// URLs. Only scheme, IP literals, and `localhost`-style names are evaluated.
pub fn check_destination_literal(
    url: &Url,
    allowed_private_hosts: &[String],
) -> Result<(), WebClientError> {
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(WebClientError::InvalidUrl {
                detail: format!("scheme '{other}' is not allowed; only http/https"),
            });
        }
    }
    match url.host() {
        Some(host) => check_host_literal(host, allowed_private_hosts),
        None => Err(WebClientError::InvalidUrl {
            detail: "URL has no host".to_string(),
        }),
    }
}

/// Full destination check for the initial request: scheme, IP literals,
/// `localhost`-style names, and — unless the request is delegated to a proxy —
/// DNS resolution of hostnames, where every resolved address must be public.
pub async fn check_destination(
    url: &Url,
    allowed_private_hosts: &[String],
    via_proxy: bool,
) -> Result<(), WebClientError> {
    let host = match url.host() {
        Some(Host::Domain(domain)) => domain.to_lowercase(),
        Some(_) => return check_destination_literal(url, allowed_private_hosts),
        None => {
            return Err(WebClientError::InvalidUrl {
                detail: "URL has no host".to_string(),
            });
        }
    };

    check_host_literal(Host::Domain(host.as_str()), allowed_private_hosts)?;

    if allowed_private_hosts
        .iter()
        .any(|entry| entry.trim() == "*")
    {
        return Ok(());
    }

    if via_proxy {
        // With a proxy the remote hop resolves DNS and owns the connection;
        // local resolution would be meaningless (and wrong for hosts only the
        // proxy can reach). Literal checks above still apply.
        return Ok(());
    }

    let port = url.port_or_known_default().unwrap_or(80);
    resolve_host_and_check_public(&host, port).await
}

/// Resolves a hostname and requires every address to be public. Exposed for
/// tests: this is the DNS-rebinding check for the initial request.
pub(crate) async fn resolve_host_and_check_public(
    host: &str,
    port: u16,
) -> Result<(), WebClientError> {
    let lookup = tokio::time::timeout(
        Duration::from_secs(DEFAULT_WEB_CONNECT_TIMEOUT_SECS),
        tokio::net::lookup_host((host, port)),
    )
    .await;
    let addresses: Vec<std::net::SocketAddr> = match lookup {
        Err(_) => {
            return Err(WebClientError::DnsFailure {
                host: host.to_string(),
                detail: "DNS resolution timed out".to_string(),
            });
        }
        Ok(Err(e)) => {
            return Err(WebClientError::DnsFailure {
                host: host.to_string(),
                detail: e.to_string(),
            });
        }
        Ok(Ok(addrs)) => addrs.collect::<Vec<_>>(),
    };
    if addresses.is_empty() {
        return Err(WebClientError::DnsFailure {
            host: host.to_string(),
            detail: "no addresses resolved".to_string(),
        });
    }
    for addr in addresses {
        if is_private_ip(addr.ip()) {
            return Err(private_blocked(host.to_string()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unified client builder
// ---------------------------------------------------------------------------

/// Everything `build_web_client` needs. Tools derive this from their own
/// settings plus the shared constants.
pub struct WebClientSettings {
    pub proxy: ProxySettings,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub max_redirects: usize,
    /// Hosts from the tool's `allowed_domains` that explicitly permit private
    /// network access; the redirect policy honors the same list.
    pub allowed_private_hosts: Vec<String>,
}

fn redirect_policy(settings: &WebClientSettings) -> reqwest::redirect::Policy {
    let allowed = settings.allowed_private_hosts.clone();
    let max = settings.max_redirects;
    reqwest::redirect::Policy::custom(move |attempt: Attempt| {
        if attempt.previous().len() >= max {
            return attempt.error(format!("exceeded {max} redirects"));
        }
        match check_destination_literal(attempt.url(), &allowed) {
            Ok(()) => attempt.follow(),
            Err(err) => attempt.error(err.to_string()),
        }
    })
}

/// Builds the one shared reqwest client flavor used by all non-browser web
/// tools. Proxy URLs are validated eagerly so misconfiguration surfaces as
/// `ProxyConfigurationInvalid` instead of a runtime connect error.
pub fn build_web_client(settings: &WebClientSettings) -> Result<Client, WebClientError> {
    let mut builder = Client::builder()
        .user_agent(WEB_USER_AGENT)
        .connect_timeout(settings.connect_timeout)
        .timeout(settings.request_timeout)
        .redirect(redirect_policy(settings));
    builder = apply_proxy_checked(builder, &settings.proxy)?;
    builder
        .build()
        .map_err(|e| WebClientError::ProxyConfigurationInvalid {
            detail: format!("failed to build HTTP client: {e}"),
        })
}

fn apply_proxy_checked(
    builder: reqwest::ClientBuilder,
    proxy: &ProxySettings,
) -> Result<reqwest::ClientBuilder, WebClientError> {
    if !proxy.is_active() {
        // OmniNova config is the single source of truth: when the proxy is not
        // enabled, ambient HTTP_PROXY-style environment variables must not
        // silently change web tool behavior.
        return Ok(builder.no_proxy());
    }

    let no_proxy = proxy.no_proxy.as_deref().and_then(NoProxy::from_string);

    let mut built: Vec<Proxy> = Vec::new();
    if let Some(url) = proxy.http_proxy.as_deref() {
        let mut entry =
            Proxy::http(url).map_err(|e| WebClientError::ProxyConfigurationInvalid {
                detail: format!(
                    "invalid proxy.http_proxy '{}': {e}",
                    redact_secrets_in_text(url)
                ),
            })?;
        entry = entry.no_proxy(no_proxy.clone());
        built.push(entry);
    }
    if let Some(url) = proxy.https_proxy.as_deref() {
        let mut entry =
            Proxy::https(url).map_err(|e| WebClientError::ProxyConfigurationInvalid {
                detail: format!(
                    "invalid proxy.https_proxy '{}': {e}",
                    redact_secrets_in_text(url)
                ),
            })?;
        entry = entry.no_proxy(no_proxy.clone());
        built.push(entry);
    }
    if let Some(url) = proxy.all_proxy.as_deref() {
        let mut entry = Proxy::all(url).map_err(|e| WebClientError::ProxyConfigurationInvalid {
            detail: format!(
                "invalid proxy.all_proxy '{}': {e}",
                redact_secrets_in_text(url)
            ),
        })?;
        entry = entry.no_proxy(no_proxy.clone());
        built.push(entry);
    }
    if built.is_empty() {
        return Err(WebClientError::ProxyConfigurationInvalid {
            detail: "proxy.enabled is true but no proxy URL is configured".to_string(),
        });
    }

    let mut out = builder;
    for proxy in built {
        out = out.proxy(proxy);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tool settings plumbing
// ---------------------------------------------------------------------------

/// Per-tool network settings derived from config at registry build time.
#[derive(Debug, Clone)]
pub struct WebToolSettings {
    pub proxy: ProxySettings,
    pub request_timeout: Duration,
    pub max_response_size: usize,
}

impl WebToolSettings {
    pub fn from_config(proxy: &ProxyConfig, timeout_secs: u64, max_response_size: usize) -> Self {
        Self {
            proxy: ProxySettings::from_config(proxy),
            request_timeout: sanitize_request_timeout(timeout_secs),
            max_response_size: if max_response_size == 0 {
                DEFAULT_WEB_MAX_RESPONSE_BYTES
            } else {
                max_response_size
            },
        }
    }

    pub fn web_client_settings(&self, allowed_private_hosts: Vec<String>) -> WebClientSettings {
        WebClientSettings {
            proxy: self.proxy.clone(),
            connect_timeout: Duration::from_secs(DEFAULT_WEB_CONNECT_TIMEOUT_SECS),
            request_timeout: self.request_timeout,
            max_redirects: WEB_MAX_REDIRECTS,
            allowed_private_hosts,
        }
    }
}

/// Zero or out-of-range configured timeouts fall back to the shared default
/// instead of producing unbounded or instant timeouts.
pub fn sanitize_request_timeout(secs: u64) -> Duration {
    let effective = if secs == 0 {
        DEFAULT_WEB_REQUEST_TIMEOUT_SECS
    } else {
        secs.clamp(1, MAX_WEB_REQUEST_TIMEOUT_SECS)
    };
    Duration::from_secs(effective)
}

// ---------------------------------------------------------------------------
// Bounded body reading
// ---------------------------------------------------------------------------

pub struct LimitedBody {
    pub bytes: Vec<u8>,
    /// Bytes actually read from the stream; when `truncated` is true this is a
    /// lower bound on the real response size because reading stops early.
    pub total_read: u64,
    pub truncated: bool,
}

/// Streams the response body while keeping at most `max_bytes` in memory.
/// Oversized responses stop reading at the boundary instead of buffering the
/// whole payload first.
pub async fn read_body_limited(
    response: &mut reqwest::Response,
    max_bytes: usize,
) -> Result<LimitedBody, WebClientError> {
    let mut buffer: Vec<u8> = Vec::with_capacity(max_bytes.min(64 * 1024));
    let mut total_read: u64 = 0;
    loop {
        let chunk = response
            .chunk()
            .await
            .map_err(|e| WebClientError::BodyReadFailed {
                detail: map_reqwest_error(e).to_string(),
            })?;
        let Some(chunk) = chunk else {
            break;
        };
        total_read += chunk.len() as u64;
        if buffer.len() < max_bytes {
            let remaining = max_bytes - buffer.len();
            let take = remaining.min(chunk.len());
            buffer.extend_from_slice(&chunk[..take]);
        }
        if buffer.len() >= max_bytes && total_read as usize > max_bytes {
            return Ok(LimitedBody {
                bytes: buffer,
                total_read,
                truncated: true,
            });
        }
    }
    let truncated = buffer.len() < total_read as usize;
    Ok(LimitedBody {
        bytes: buffer,
        total_read,
        truncated,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::tools::http_request::HttpRequestTool;
    use crate::tools::traits::Tool;
    use crate::tools::web_fetch::WebFetchTool;
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::sync::Arc;

    // -- local test server --------------------------------------------------

    #[derive(Debug)]
    pub(crate) struct CapturedRequest {
        pub request_line: String,
        #[allow(dead_code)]
        pub headers: Vec<String>,
        #[allow(dead_code)]
        pub body: Vec<u8>,
    }

    pub(crate) fn write_response(
        stream: &mut TcpStream,
        status: &str,
        body: &str,
        extra: &[String],
    ) {
        let mut response = format!(
            "{status}\r\ncontent-length: {}\r\nconnection: keep-alive\r\n",
            body.len()
        );
        for header in extra {
            response.push_str(header);
            response.push_str("\r\n");
        }
        response.push_str("\r\n");
        response.push_str(body);
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    /// Spawns a throwaway HTTP/1.1 server on 127.0.0.1:0. The handler receives
    /// each captured request and writes a raw response into the stream.
    pub(crate) fn spawn_test_server<F>(handler: F) -> u16
    where
        F: Fn(CapturedRequest, &mut TcpStream) + Send + Sync + 'static,
    {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let port = listener.local_addr().unwrap().port();
        let handler = Arc::new(handler);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let handler = handler.clone();
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
                    loop {
                        let mut request_line = String::new();
                        match reader.read_line(&mut request_line) {
                            Ok(0) | Err(_) => break,
                            Ok(_) => {}
                        }
                        if request_line.trim().is_empty() {
                            continue;
                        }
                        let mut headers = Vec::new();
                        let mut content_length = 0usize;
                        loop {
                            let mut header_line = String::new();
                            match reader.read_line(&mut header_line) {
                                Ok(0) | Err(_) => break,
                                Ok(_) => {}
                            }
                            let trimmed = header_line.trim_end().to_string();
                            if trimmed.is_empty() {
                                break;
                            }
                            let lowered = trimmed.to_lowercase();
                            if let Some(rest) = lowered.strip_prefix("content-length:") {
                                content_length = rest.trim().parse().unwrap_or(0);
                            }
                            headers.push(trimmed);
                        }
                        let mut body = vec![0u8; content_length];
                        if content_length > 0 {
                            let _ = reader.read_exact(&mut body);
                        }
                        handler(
                            CapturedRequest {
                                request_line: request_line.trim_end().to_string(),
                                headers,
                                body,
                            },
                            &mut stream,
                        );
                    }
                });
            }
        });
        port
    }

    fn tool_settings(
        request_timeout: Duration,
        max_response_size: usize,
        proxy: ProxySettings,
    ) -> WebToolSettings {
        WebToolSettings {
            proxy,
            request_timeout,
            max_response_size,
        }
    }

    fn default_tool_settings() -> WebToolSettings {
        tool_settings(Duration::from_secs(5), 1_048_576, ProxySettings::default())
    }

    // -- private network classification -------------------------------------

    #[test]
    fn public_ip_literals_pass() {
        for url in [
            "http://1.1.1.1/",
            "http://8.8.8.8:8080/x",
            "http://[2606:4700::1]/",
        ] {
            let parsed = Url::parse(url).unwrap();
            assert!(
                check_destination_literal(&parsed, &[]).is_ok(),
                "{url} must pass"
            );
        }
    }

    #[test]
    fn private_ip_literals_blocked() {
        let blocked = [
            "http://127.0.0.1/",
            "http://127.1/", // IPv4 shorthand canonicalizes to 127.0.0.1
            "http://127.254.1.2/",
            "http://10.1.2.3/",
            "http://172.16.0.1/",
            "http://172.31.255.255/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/", // cloud metadata endpoint
            "http://0.0.0.0/",
            "http://100.64.0.1/", // shared address space
            "http://198.18.0.1/", // benchmarking range
            "http://[::1]/",
            "http://[::ffff:127.0.0.1]/", // IPv4-mapped IPv6 loopback
            "http://[::ffff:10.0.0.1]/",
            "http://[fe80::1]/",
            "http://[fc00::1]/",
            "http://[fd12:3456::1]/",
        ];
        for url in blocked {
            let parsed = Url::parse(url).unwrap();
            let err = match check_destination_literal(&parsed, &[]) {
                Ok(()) => panic!("{url} unexpectedly passed the private-network check"),
                Err(err) => err,
            };
            assert!(
                matches!(err, WebClientError::PrivateNetworkBlocked { .. }),
                "{url} must be blocked, got {err:?}"
            );
        }
    }

    #[test]
    fn localhost_names_blocked() {
        for url in [
            "http://localhost/",
            "http://localhost:3000/",
            "http://sub.localhost/",
            "http://LOCALHOST/x",
        ] {
            let parsed = Url::parse(url).unwrap();
            let err = check_destination_literal(&parsed, &[]).unwrap_err();
            assert!(matches!(err, WebClientError::PrivateNetworkBlocked { .. }));
        }
    }

    #[test]
    fn explicit_allowlist_only_unblocks_listed_host() {
        let allowed = vec!["localhost".to_string()];
        assert!(check_destination_literal(
            &Url::parse("http://localhost:3000/").unwrap(),
            &allowed
        )
        .is_ok());
        // A listed private host does not unlock other private targets.
        assert!(
            check_destination_literal(&Url::parse("http://127.0.0.1/").unwrap(), &allowed).is_err()
        );
        assert!(check_destination_literal(
            &Url::parse("http://169.254.169.254/").unwrap(),
            &allowed
        )
        .is_err());
    }

    #[tokio::test]
    async fn full_access_wildcard_allows_private_network_without_weakening_default() {
        let url = Url::parse("http://127.0.0.1:9/").unwrap();
        assert!(check_destination(&url, &[], false).await.is_err());
        assert!(check_destination(&url, &["*".to_string()], false)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn full_access_wildcard_executes_local_network_request() {
        let port = spawn_test_server(|_req, stream| {
            write_response(stream, "HTTP/1.1 200 OK", "full-access-network-ok", &[]);
        });
        let tool = HttpRequestTool::new(vec!["*".to_string()], default_tool_settings());

        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/health")}))
            .await
            .unwrap();

        assert!(result.success, "request failed: {:?}", result.error);
        assert!(result.output.starts_with("HTTP 200"));
        assert!(result.output.contains("full-access-network-ok"));
    }

    #[test]
    fn non_http_schemes_rejected() {
        for url in [
            "file:///etc/passwd",
            "ftp://example.com/",
            "data:text/plain,hi",
        ] {
            let parsed = Url::parse(url).unwrap();
            let err = check_destination_literal(&parsed, &[]).unwrap_err();
            assert!(matches!(err, WebClientError::InvalidUrl { .. }), "{url}");
        }
    }

    #[tokio::test]
    async fn hostname_resolving_to_private_ip_blocked() {
        // "localhost" resolves to 127.0.0.1 via the system resolver; bypassing
        // the literal fast-path exercises the actual DNS-based rebinding check.
        let err = resolve_host_and_check_public("localhost", 80)
            .await
            .unwrap_err();
        assert!(matches!(err, WebClientError::PrivateNetworkBlocked { .. }));
    }

    #[tokio::test]
    async fn dns_failure_is_classified() {
        let parsed = Url::parse("http://omninova-definitely-not-real-w24.invalid/").unwrap();
        let err = check_destination(&parsed, &[], false).await.unwrap_err();
        assert!(
            matches!(err, WebClientError::DnsFailure { .. }),
            "expected DnsFailure, got {err:?}"
        );
    }

    #[test]
    fn allowlist_uses_dot_delimited_host_boundary() {
        let allowed = vec!["example.com".to_string()];
        assert!(host_matches_allowlist("example.com", &allowed));
        assert!(host_matches_allowlist("www.example.com", &allowed));
        assert!(host_matches_allowlist("a.b.example.com", &allowed));
        assert!(!host_matches_allowlist("evil-example.com", &allowed));
        assert!(!host_matches_allowlist("notexample.com", &allowed));
        assert!(!host_matches_allowlist("example.com.evil.test", &allowed));
        assert!(host_matches_allowlist("anything.test", &[]));
    }

    #[test]
    fn url_userinfo_is_redacted() {
        let url = Url::parse("https://user:secret@example.com/path?q=1").unwrap();
        let redacted = redact_url(&url);
        assert!(redacted.contains("***:***@example.com"));
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("user:secret"));
        let embedded = redact_secrets_in_text(
            "failed connecting to https://alice:hunter2@proxy.example:8080/x",
        );
        assert!(embedded.contains("***:***@proxy.example"));
        assert!(!embedded.contains("hunter2"));
    }

    // -- proxy settings ------------------------------------------------------

    #[test]
    fn proxy_settings_from_config() {
        let cfg = crate::config::schema::ProxyConfig {
            enabled: true,
            http_proxy: Some(" http://proxy:3128 ".into()),
            ..Default::default()
        };
        let settings = ProxySettings::from_config(&cfg);
        assert_eq!(settings.http_proxy.as_deref(), Some("http://proxy:3128"));
        assert_eq!(settings.for_scheme("http"), Some("http://proxy:3128"));
        assert_eq!(settings.for_scheme("https"), None);

        let disabled = ProxySettings {
            enabled: false,
            http_proxy: Some("http://proxy:3128".into()),
            ..Default::default()
        };
        assert_eq!(disabled.for_scheme("http"), None);
        assert!(!disabled.is_active());
    }

    #[test]
    fn invalid_proxy_url_fails_build() {
        let settings = WebClientSettings {
            proxy: ProxySettings {
                enabled: true,
                all_proxy: Some("not a proxy url".into()),
                ..Default::default()
            },
            connect_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(1),
            max_redirects: 10,
            allowed_private_hosts: Vec::new(),
        };
        let err = build_web_client(&settings).unwrap_err();
        assert!(matches!(
            err,
            WebClientError::ProxyConfigurationInvalid { .. }
        ));
    }

    #[test]
    fn proxy_url_with_credentials_builds() {
        let settings = WebClientSettings {
            proxy: ProxySettings {
                enabled: true,
                all_proxy: Some("http://user:pass@127.0.0.1:9".into()),
                ..Default::default()
            },
            connect_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(1),
            max_redirects: 10,
            allowed_private_hosts: Vec::new(),
        };
        assert!(build_web_client(&settings).is_ok());
    }

    // -- timeout sanitization ------------------------------------------------

    #[test]
    fn request_timeout_sanitized() {
        assert_eq!(sanitize_request_timeout(0), Duration::from_secs(30));
        assert_eq!(sanitize_request_timeout(5), Duration::from_secs(5));
        assert_eq!(
            sanitize_request_timeout(100_000),
            Duration::from_secs(MAX_WEB_REQUEST_TIMEOUT_SECS)
        );
    }

    // -- raw client behavior on local servers --------------------------------

    #[tokio::test]
    async fn redirects_within_public_network_are_followed() {
        let port = spawn_test_server(|req, stream| {
            if req.request_line.starts_with("GET /final") {
                write_response(stream, "HTTP/1.1 200 OK", "final", &[]);
            } else {
                write_response(
                    stream,
                    "HTTP/1.1 302 Found",
                    "",
                    &["location: /final".to_string()],
                );
            }
        });
        let settings = WebClientSettings {
            proxy: ProxySettings::default(),
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(5),
            max_redirects: WEB_MAX_REDIRECTS,
            allowed_private_hosts: vec!["127.0.0.1".to_string()],
        };
        let client = build_web_client(&settings).unwrap();
        let mut response = client
            .get(Url::parse(&format!("http://127.0.0.1:{port}/start")).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        let body = response.text().await.unwrap();
        assert_eq!(body, "final");
    }

    #[tokio::test]
    async fn redirect_to_private_network_is_rejected() {
        let port = spawn_test_server(|_req, stream| {
            write_response(
                stream,
                "HTTP/1.1 302 Found",
                "",
                &["location: http://127.0.0.1:9/private".to_string()],
            );
        });
        // No explicit private allowlist: the redirect must be stopped even
        // though the initial URL connected fine.
        let settings = WebClientSettings {
            proxy: ProxySettings::default(),
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(5),
            max_redirects: WEB_MAX_REDIRECTS,
            allowed_private_hosts: Vec::new(),
        };
        let client = build_web_client(&settings).unwrap();
        let err = client
            .get(Url::parse(&format!("http://127.0.0.1:{port}/start")).unwrap())
            .send()
            .await
            .unwrap_err();
        let mapped = map_reqwest_error(err);
        assert!(
            matches!(mapped, WebClientError::PrivateNetworkBlocked { .. }),
            "expected PrivateNetworkBlocked, got {mapped}"
        );
    }

    #[tokio::test]
    async fn redirect_loop_hits_limit() {
        let port = spawn_test_server(|_req, stream| {
            write_response(
                stream,
                "HTTP/1.1 302 Found",
                "",
                &["location: /loop".to_string()],
            );
        });
        let settings = WebClientSettings {
            proxy: ProxySettings::default(),
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(5),
            max_redirects: WEB_MAX_REDIRECTS,
            allowed_private_hosts: vec!["127.0.0.1".to_string()],
        };
        let client = build_web_client(&settings).unwrap();
        let err = client
            .get(Url::parse(&format!("http://127.0.0.1:{port}/loop")).unwrap())
            .send()
            .await
            .unwrap_err();
        let mapped = map_reqwest_error(err);
        assert!(
            matches!(mapped, WebClientError::RedirectError { .. }),
            "expected RedirectError, got {mapped}"
        );
    }

    #[tokio::test]
    async fn slow_response_hits_request_timeout() {
        let port = spawn_test_server(|_req, stream| {
            std::thread::sleep(Duration::from_secs(2));
            write_response(stream, "HTTP/1.1 200 OK", "late", &[]);
        });
        let settings = WebClientSettings {
            proxy: ProxySettings::default(),
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_millis(300),
            max_redirects: WEB_MAX_REDIRECTS,
            allowed_private_hosts: vec!["127.0.0.1".to_string()],
        };
        let client = build_web_client(&settings).unwrap();
        let err = client
            .get(Url::parse(&format!("http://127.0.0.1:{port}/slow")).unwrap())
            .send()
            .await
            .unwrap_err();
        let mapped = map_reqwest_error(err);
        assert!(
            matches!(mapped, WebClientError::RequestTimeout { .. }),
            "expected RequestTimeout, got {mapped}"
        );
    }

    #[tokio::test]
    async fn unreachable_host_hits_connect_timeout() {
        let settings = WebClientSettings {
            proxy: ProxySettings::default(),
            connect_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(10),
            max_redirects: WEB_MAX_REDIRECTS,
            allowed_private_hosts: vec!["10.255.255.1".to_string()],
        };
        let client = build_web_client(&settings).unwrap();
        let result = client
            .get(Url::parse("http://10.255.255.1:9/").unwrap())
            .send()
            .await;
        // Non-routable address: must fail inside the 1s connect timeout. Some
        // platforms report it as an immediate unreachable instead.
        if let Err(err) = result {
            let mapped = map_reqwest_error(err);
            assert!(
                matches!(
                    mapped,
                    WebClientError::ConnectionTimeout { .. }
                        | WebClientError::ConnectionFailed { .. }
                ),
                "expected connect failure, got {mapped}"
            );
        }
    }

    #[tokio::test]
    async fn oversized_response_is_bounded() {
        let big = "x".repeat(2 * 1024 * 1024);
        let port = spawn_test_server(move |_req, stream| {
            write_response(stream, "HTTP/1.1 200 OK", &big, &[]);
        });
        let settings = WebClientSettings {
            proxy: ProxySettings::default(),
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(10),
            max_redirects: WEB_MAX_REDIRECTS,
            allowed_private_hosts: vec!["127.0.0.1".to_string()],
        };
        let client = build_web_client(&settings).unwrap();
        let mut response = client
            .get(Url::parse(&format!("http://127.0.0.1:{port}/big")).unwrap())
            .send()
            .await
            .unwrap();
        let body = read_body_limited(&mut response, 1024 * 1024).await.unwrap();
        assert!(body.truncated);
        assert_eq!(body.bytes.len(), 1024 * 1024);
    }

    #[tokio::test]
    async fn proxy_is_actually_used() {
        let origin = spawn_test_server(|_req, stream| {
            write_response(stream, "HTTP/1.1 200 OK", "origin-direct", &[]);
        });
        let proxy_port = spawn_test_server(move |req, stream| {
            // Absolute-form request line proves the client treated us as a proxy.
            assert!(
                req.request_line
                    .contains(&format!("http://127.0.0.1:{origin}/")),
                "proxy did not receive absolute-form request: {}",
                req.request_line
            );
            write_response(stream, "HTTP/1.1 200 OK", "via-proxy", &[]);
        });
        let settings = WebClientSettings {
            proxy: ProxySettings {
                enabled: true,
                all_proxy: Some(format!("http://127.0.0.1:{proxy_port}")),
                ..Default::default()
            },
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(5),
            max_redirects: WEB_MAX_REDIRECTS,
            allowed_private_hosts: Vec::new(),
        };
        let client = build_web_client(&settings).unwrap();
        let response = client
            .get(Url::parse(&format!("http://127.0.0.1:{origin}/resource")).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.text().await.unwrap(), "via-proxy");
    }

    #[tokio::test]
    async fn no_proxy_bypasses_mock_proxy() {
        let origin = spawn_test_server(|_req, stream| {
            write_response(stream, "HTTP/1.1 200 OK", "origin-direct", &[]);
        });
        let target = format!("http://127.0.0.1:{origin}/");
        let proxy_target = target.clone();
        let proxy_port = spawn_test_server(move |req, stream| {
            assert!(
                !req.request_line.contains(&proxy_target),
                "request unexpectedly reached the proxy: {}",
                req.request_line
            );
            write_response(stream, "HTTP/1.1 200 OK", "via-proxy", &[]);
        });
        let settings = WebClientSettings {
            proxy: ProxySettings {
                enabled: true,
                all_proxy: Some(format!("http://127.0.0.1:{proxy_port}")),
                no_proxy: Some("127.0.0.1".into()),
                ..Default::default()
            },
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(5),
            max_redirects: WEB_MAX_REDIRECTS,
            allowed_private_hosts: Vec::new(),
        };
        let client = build_web_client(&settings).unwrap();
        let response = client
            .get(Url::parse(&target).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "origin-direct");
    }

    // -- tool-level end-to-end -----------------------------------------------

    #[tokio::test]
    async fn web_fetch_strips_html_from_local_server() {
        let port = spawn_test_server(|_req, stream| {
            write_response(
                stream,
                "HTTP/1.1 200 OK",
                "<html><body><h1>hello world</h1></body></html>",
                &["content-type: text/html".to_string()],
            );
        });
        let tool = WebFetchTool::new(vec!["127.0.0.1".to_string()], default_tool_settings());
        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/page")}))
            .await
            .unwrap();
        assert!(result.success, "fetch failed: {:?}", result.error);
        assert!(
            result.output.contains("# hello world"),
            "output={}",
            result.output
        );
    }

    #[tokio::test]
    async fn web_fetch_blocks_private_without_explicit_allow() {
        let port = spawn_test_server(|_req, stream| {
            write_response(stream, "HTTP/1.1 200 OK", "secret", &[]);
        });
        let tool = WebFetchTool::new(Vec::new(), default_tool_settings());
        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/page")}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("[PrivateNetworkBlocked]"));
    }

    #[tokio::test]
    async fn web_fetch_localhost_blocked_even_with_ip_allowed() {
        let port = spawn_test_server(|_req, stream| {
            write_response(stream, "HTTP/1.1 200 OK", "secret", &[]);
        });
        // Allowlisting the IP must not implicitly unlock the "localhost"
        // name: access stays blocked (by the per-host domain gate here).
        let tool = WebFetchTool::new(vec!["127.0.0.1".to_string()], default_tool_settings());
        let result = tool
            .execute(json!({"url": format!("http://localhost:{port}/page")}))
            .await
            .unwrap();
        assert!(!result.success);
        let error = result.error.unwrap();
        assert!(
            error.contains("[PrivateNetworkBlocked]") || error.contains("not in allowed list"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn web_fetch_http_status_error() {
        let port = spawn_test_server(|_req, stream| {
            write_response(stream, "HTTP/1.1 404 Not Found", "nope", &[]);
        });
        let tool = WebFetchTool::new(vec!["127.0.0.1".to_string()], default_tool_settings());
        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/missing")}))
            .await
            .unwrap();
        assert!(!result.success);
        assert_eq!(result.error.unwrap(), "[HttpStatusError] HTTP 404");
    }

    #[tokio::test]
    async fn web_fetch_timeout_is_enforced() {
        let port = spawn_test_server(|_req, stream| {
            std::thread::sleep(Duration::from_secs(2));
            write_response(stream, "HTTP/1.1 200 OK", "late", &[]);
        });
        let tool = WebFetchTool::new(
            vec!["127.0.0.1".to_string()],
            tool_settings(
                Duration::from_millis(300),
                1_048_576,
                ProxySettings::default(),
            ),
        );
        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/slow")}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("[RequestTimeout]"));
    }

    #[tokio::test]
    async fn web_fetch_response_truncated_at_limit() {
        let big = "y".repeat(2 * 1024 * 1024);
        let port = spawn_test_server(move |_req, stream| {
            write_response(
                stream,
                "HTTP/1.1 200 OK",
                &big,
                &["content-type: text/plain".to_string()],
            );
        });
        let tool = WebFetchTool::new(
            vec!["127.0.0.1".to_string()],
            tool_settings(
                Duration::from_secs(10),
                1024 * 1024,
                ProxySettings::default(),
            ),
        );
        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/big")}))
            .await
            .unwrap();
        assert!(result.success);
        // The model-visible char budget applies before the network marker,
        // so the model sees the char-truncation notice instead.
        assert!(
            result
                .output
                .contains("[content truncated: showing 40,000 of"),
            "output tail={}",
            &result.output[result.output.len().saturating_sub(200)..]
        );
        assert!(result.output.len() < 50_000);
    }

    #[tokio::test]
    async fn web_fetch_follows_redirect_within_allowlist() {
        let port = spawn_test_server(|req, stream| {
            if req.request_line.starts_with("GET /final") {
                write_response(
                    stream,
                    "HTTP/1.1 200 OK",
                    "<p>final page</p>",
                    &["content-type: text/html".to_string()],
                );
            } else {
                write_response(
                    stream,
                    "HTTP/1.1 302 Found",
                    "",
                    &["location: /final".to_string()],
                );
            }
        });
        let tool = WebFetchTool::new(vec!["127.0.0.1".to_string()], default_tool_settings());
        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/start")}))
            .await
            .unwrap();
        assert!(result.success, "fetch failed: {:?}", result.error);
        assert!(
            result.output.contains("final page"),
            "output={}",
            result.output
        );
    }

    #[tokio::test]
    async fn web_fetch_output_has_title_and_final_url_metadata() {
        let port = spawn_test_server(|req, stream| {
            if req.request_line.starts_with("GET /final") {
                write_response(
                    stream,
                    "HTTP/1.1 200 OK",
                    "<html><head><title>Meta Test</title></head><body><p>body text</p></body></html>",
                    &["content-type: text/html".to_string()],
                );
            } else {
                write_response(
                    stream,
                    "HTTP/1.1 302 Found",
                    "",
                    &["location: /final".to_string()],
                );
            }
        });
        let tool = WebFetchTool::new(vec!["127.0.0.1".to_string()], default_tool_settings());
        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/start")}))
            .await
            .unwrap();
        assert!(result.success, "fetch failed: {:?}", result.error);
        assert!(
            result
                .output
                .starts_with("Web content from:\nTitle: Meta Test\n"),
            "output={}",
            result.output
        );
        assert!(result.output.contains("--- BEGIN WEB CONTENT ---"));
        assert!(
            result
                .output
                .contains(&format!("URL: http://127.0.0.1:{port}/final")),
            "final URL header missing: {}",
            result.output
        );
        assert!(result.output.contains("body text"));
    }

    #[tokio::test]
    async fn http_request_get_and_post_roundtrip() {
        let port = spawn_test_server(|req, stream| {
            if req.request_line.starts_with("POST /echo") {
                let echoed = String::from_utf8_lossy(&req.body).to_string();
                write_response(stream, "HTTP/1.1 201 Created", &echoed, &[]);
            } else {
                write_response(stream, "HTTP/1.1 200 OK", "get-ok", &[]);
            }
        });
        let tool = HttpRequestTool::new(vec!["127.0.0.1".to_string()], default_tool_settings());

        let get = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/get")}))
            .await
            .unwrap();
        assert!(get.success, "GET failed: {:?}", get.error);
        assert!(get.output.starts_with("HTTP 200"));
        assert!(get.output.contains("get-ok"));

        let post = tool
            .execute(json!({
                "url": format!("http://127.0.0.1:{port}/echo"),
                "method": "POST",
                "headers": {"X-Test": "w24"},
                "body": "payload-w24"
            }))
            .await
            .unwrap();
        assert!(post.success, "POST failed: {:?}", post.error);
        assert!(post.output.starts_with("HTTP 201"));
        assert!(post.output.contains("payload-w24"));
    }

    #[tokio::test]
    async fn http_request_rejects_forbidden_headers() {
        // Empty allowlist: this test exercises local header validation, which
        // runs before any destination check.
        let tool = HttpRequestTool::new(Vec::new(), default_tool_settings());
        for header in [
            "Host",
            "Content-Length",
            "Transfer-Encoding",
            "Proxy-Authorization",
        ] {
            let result = tool
                .execute(json!({
                    "url": "http://example.com/",
                    "headers": { header: "evil" }
                }))
                .await
                .unwrap();
            assert!(
                result
                    .error
                    .unwrap_or_default()
                    .contains("[HeaderRejected]"),
                "header {header} must be rejected"
            );
        }
        let result = tool
            .execute(json!({
                "url": "http://example.com/",
                "headers": { "Bad Header": "v" }
            }))
            .await
            .unwrap();
        assert!(result.error.unwrap().contains("[HeaderRejected]"));
    }

    #[tokio::test]
    async fn http_request_rejects_oversized_body() {
        let tool = HttpRequestTool::new(Vec::new(), default_tool_settings());
        let result = tool
            .execute(json!({
                "url": "http://example.com/upload",
                "method": "POST",
                "body": "x".repeat(2 * 1024 * 1024)
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("[RequestTooLarge]"));
    }

    #[tokio::test]
    async fn http_request_blocks_private_without_explicit_allow() {
        let port = spawn_test_server(|_req, stream| {
            write_response(stream, "HTTP/1.1 200 OK", "secret", &[]);
        });
        let tool = HttpRequestTool::new(Vec::new(), default_tool_settings());
        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/get")}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("[PrivateNetworkBlocked]"));
    }

    #[tokio::test]
    async fn http_request_unknown_method_rejected() {
        let tool = HttpRequestTool::new(Vec::new(), default_tool_settings());
        let result = tool
            .execute(json!({
                "url": "http://example.com/",
                "method": "TRACE"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not supported"));
    }

    #[tokio::test]
    async fn http_request_error_status_marks_failure() {
        let port = spawn_test_server(|_req, stream| {
            write_response(stream, "HTTP/1.1 500 Internal Server Error", "boom", &[]);
        });
        let tool = HttpRequestTool::new(vec!["127.0.0.1".to_string()], default_tool_settings());
        let result = tool
            .execute(json!({"url": format!("http://127.0.0.1:{port}/err")}))
            .await
            .unwrap();
        assert!(!result.success);
        assert_eq!(result.error.unwrap(), "[HttpStatusError] HTTP 500");
    }

    #[tokio::test]
    async fn web_search_unreachable_api_maps_error() {
        // No real Brave call: point the settings at an unroutable proxy so the
        // request fails fast and proves the error mapping path.
        let tool = crate::tools::web_search::WebSearchTool::new(
            "unused-key",
            tool_settings(
                Duration::from_secs(2),
                DEFAULT_WEB_MAX_RESPONSE_BYTES,
                ProxySettings {
                    enabled: true,
                    all_proxy: Some("http://127.0.0.1:9".into()),
                    ..Default::default()
                },
            ),
        );
        let result = tool.execute(json!({"query": "test"})).await.unwrap();
        assert!(!result.success);
        let error = result.error.unwrap();
        assert!(
            error.starts_with("[ProxyConnectionFailed]")
                || error.starts_with("[ConnectionFailed]")
                || error.starts_with("[ConnectionTimeout]")
                || error.starts_with("[RequestTimeout]"),
            "unexpected error: {error}"
        );
    }

    // -- real network ---------------------------------------------------------

    #[tokio::test]
    #[ignore = "requires internet access; run explicitly once per release"]
    async fn real_public_fetch_example_com() {
        let tool = WebFetchTool::new(Vec::new(), default_tool_settings());
        let result = tool
            .execute(json!({"url": "https://example.com/"}))
            .await
            .unwrap();
        assert!(result.success, "real fetch failed: {:?}", result.error);
        assert!(result.output.to_lowercase().contains("example domain"));
    }
}
