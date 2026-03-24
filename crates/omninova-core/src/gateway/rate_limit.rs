//! Rate Limiting Middleware for HTTP Gateway
//!
//! Provides request rate limiting to prevent API abuse.
//!
//! Features:
//! - Token bucket algorithm with configurable limits
//! - Per-IP and per-API-key rate limiting
//! - Rate limit headers in responses
//! - Graceful handling of limit exceeded
//!
//! [Source: Story 8.3 - API 认证与授权]

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

// ============================================================================
// Error Types
// ============================================================================

/// Rate limit error
#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded")]
    LimitExceeded {
        retry_after: u64,
        limit: u32,
        remaining: u32,
    },
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        match &self {
            RateLimitError::LimitExceeded {
                retry_after,
                limit,
                remaining,
            } => {
                let body = Json(serde_json::json!({
                    "success": false,
                    "error": {
                        "code": "RATE_LIMITED",
                        "message": "Rate limit exceeded. Please retry later.",
                        "details": {
                            "retry_after_seconds": retry_after,
                            "limit": limit,
                            "remaining": remaining
                        }
                    }
                }));

                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [
                        ("X-RateLimit-Limit", limit.to_string()),
                        ("X-RateLimit-Remaining", remaining.to_string()),
                        ("X-RateLimit-Reset", retry_after.to_string()),
                        ("Retry-After", retry_after.to_string()),
                    ],
                    body,
                )
                    .into_response()
            }
        }
    }
}

// ============================================================================
// Rate Limit Configuration
// ============================================================================

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per minute
    pub requests_per_minute: u32,
    /// Burst size (additional requests allowed in short bursts)
    pub burst_size: u32,
    /// Whether to enable per-IP rate limiting for unauthenticated requests
    pub limit_unauthenticated: bool,
    /// Max requests per minute for unauthenticated requests
    pub unauthenticated_limit: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 100,
            burst_size: 20,
            limit_unauthenticated: true,
            unauthenticated_limit: 10,
        }
    }
}

// ============================================================================
// Rate Limit Entry
// ============================================================================

/// Internal rate limit tracking entry
struct RateLimitEntry {
    /// Token bucket (current count)
    tokens: AtomicU32,
    /// Last update time
    last_update: std::sync::Mutex<Instant>,
    /// Window duration
    window: Duration,
    /// Maximum tokens
    max_tokens: u32,
}

impl RateLimitEntry {
    fn new(max_tokens: u32, window: Duration) -> Self {
        Self {
            tokens: AtomicU32::new(max_tokens),
            last_update: std::sync::Mutex::new(Instant::now()),
            window,
            max_tokens,
        }
    }

    /// Try to consume a token, returns remaining count or error
    fn try_consume(&self) -> Result<u32, ()> {
        let tokens = self.tokens.load(Ordering::Relaxed);

        if tokens == 0 {
            return Err(());
        }

        // Try to decrement atomically
        match self.tokens.compare_exchange(
            tokens,
            tokens.saturating_sub(1),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => Ok(tokens.saturating_sub(1)),
            Err(_) => {
                // Another thread got there first, try again
                self.try_consume()
            }
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&self) {
        let mut last_update = self.last_update.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(*last_update);

        // Calculate how many tokens to add based on elapsed time
        let tokens_to_add = (elapsed.as_secs_f64() / self.window.as_secs_f64()
            * self.max_tokens as f64)
            .floor() as u32;

        if tokens_to_add > 0 {
            let current = self.tokens.load(Ordering::Relaxed);
            let new_count = (current + tokens_to_add).min(self.max_tokens);
            self.tokens.store(new_count, Ordering::Relaxed);
            *last_update = now;
        }
    }
}

// ============================================================================
// Rate Limiter
// ============================================================================

/// In-memory rate limiter using token bucket algorithm
pub struct RateLimiter {
    /// Per-IP rate limit entries
    ip_limits: Arc<RwLock<HashMap<String, Arc<RateLimitEntry>>>>,
    /// Per-API-key rate limit entries
    key_limits: Arc<RwLock<HashMap<i64, Arc<RateLimitEntry>>>>,
    /// Configuration
    config: RateLimitConfig,
    /// Cleanup interval
    cleanup_interval: Duration,
    /// Last cleanup time
    last_cleanup: std::sync::Mutex<Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            ip_limits: Arc::new(RwLock::new(HashMap::new())),
            key_limits: Arc::new(RwLock::new(HashMap::new())),
            config,
            cleanup_interval: Duration::from_secs(60),
            last_cleanup: std::sync::Mutex::new(Instant::now()),
        }
    }

    /// Check rate limit for an IP address
    pub async fn check_ip(&self, ip: &str) -> Result<RateLimitStatus, RateLimitError> {
        // Periodic cleanup
        self.maybe_cleanup();

        let limits = self.ip_limits.read().await;
        if let Some(entry) = limits.get(ip) {
            entry.refill();
            let remaining = entry.try_consume().map_err(|_| {
                self.make_error(entry.max_tokens, 0)
            })?;
            return Ok(RateLimitStatus {
                limit: entry.max_tokens,
                remaining,
                reset_after: self.config.requests_per_minute,
            });
        }
        drop(limits);

        // Create new entry
        let entry = Arc::new(RateLimitEntry::new(
            self.config.unauthenticated_limit,
            Duration::from_secs(60),
        ));
        let remaining = entry.try_consume().map_err(|_| {
            self.make_error(entry.max_tokens, 0)
        })?;

        let mut limits = self.ip_limits.write().await;
        limits.insert(ip.to_string(), entry.clone());

        Ok(RateLimitStatus {
            limit: entry.max_tokens,
            remaining,
            reset_after: self.config.requests_per_minute,
        })
    }

    /// Check rate limit for an API key
    pub async fn check_api_key(&self, key_id: i64) -> Result<RateLimitStatus, RateLimitError> {
        // Periodic cleanup
        self.maybe_cleanup();

        let limits = self.key_limits.read().await;
        if let Some(entry) = limits.get(&key_id) {
            entry.refill();
            let remaining = entry.try_consume().map_err(|_| {
                self.make_error(entry.max_tokens, 0)
            })?;
            return Ok(RateLimitStatus {
                limit: entry.max_tokens,
                remaining,
                reset_after: self.config.requests_per_minute,
            });
        }
        drop(limits);

        // Create new entry
        let entry = Arc::new(RateLimitEntry::new(
            self.config.requests_per_minute + self.config.burst_size,
            Duration::from_secs(60),
        ));
        let remaining = entry.try_consume().map_err(|_| {
            self.make_error(entry.max_tokens, 0)
        })?;

        let mut limits = self.key_limits.write().await;
        limits.insert(key_id, entry.clone());

        Ok(RateLimitStatus {
            limit: entry.max_tokens,
            remaining,
            reset_after: self.config.requests_per_minute,
        })
    }

    /// Create a rate limit error
    fn make_error(&self, limit: u32, remaining: u32) -> RateLimitError {
        RateLimitError::LimitExceeded {
            retry_after: 60, // Default retry after 60 seconds
            limit,
            remaining,
        }
    }

    /// Periodic cleanup of stale entries
    fn maybe_cleanup(&self) {
        let mut last_cleanup = self.last_cleanup.lock().unwrap();
        let now = Instant::now();

        if now.duration_since(*last_cleanup) >= self.cleanup_interval {
            *last_cleanup = now;
            // In a real implementation, we would remove entries that haven't been used recently
            // For simplicity, we just note that cleanup should happen
            tracing::debug!("Rate limiter cleanup triggered");
        }
    }

    /// Get rate limit headers for a response
    pub fn get_headers(&self, status: &RateLimitStatus) -> HashMap<&'static str, String> {
        let mut headers = HashMap::new();
        headers.insert("X-RateLimit-Limit", status.limit.to_string());
        headers.insert("X-RateLimit-Remaining", status.remaining.to_string());
        headers.insert("X-RateLimit-Reset", status.reset_after.to_string());
        headers
    }
}

// ============================================================================
// Rate Limit Status
// ============================================================================

/// Rate limit status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    /// Maximum requests per window
    pub limit: u32,
    /// Remaining requests in current window
    pub remaining: u32,
    /// Seconds until window resets
    pub reset_after: u32,
}

// ============================================================================
// Axum Middleware
// ============================================================================

/// Extract client IP from request headers
fn extract_client_ip(headers: &HeaderMap) -> String {
    // Try X-Forwarded-For first
    if let Some(forwarded) = headers.get("X-Forwarded-For") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            // Take the first IP in the chain (original client)
            if let Some(ip) = forwarded_str.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }

    // Try X-Real-IP
    if let Some(real_ip) = headers.get("X-Real-IP") {
        if let Ok(ip) = real_ip.to_str() {
            return ip.to_string();
        }
    }

    // Fallback
    "unknown".to_string()
}

/// Rate limiting middleware
///
/// This middleware checks rate limits before allowing requests through.
/// It uses different limits for authenticated vs unauthenticated requests.
pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    request: Request,
    next: Next,
) -> Result<Response, RateLimitError> {
    let headers = request.headers().clone();

    // Check if authenticated (has AuthContext in extensions)
    let status = if let Some(auth_ctx) = request.extensions().get::<super::auth::AuthContext>() {
        // Authenticated - use API key rate limit
        limiter.check_api_key(auth_ctx.api_key_id).await?
    } else {
        // Unauthenticated - use IP rate limit
        let ip = extract_client_ip(&headers);
        limiter.check_ip(&ip).await?
    };

    // Add rate limit headers to response
    let mut response = next.run(request).await;

    let headers = limiter.get_headers(&status);
    for (key, value) in headers {
        if let Ok(header_name) = axum::http::header::HeaderName::from_bytes(key.as_bytes()) {
            if let Ok(header_value) = axum::http::header::HeaderValue::from_str(&value) {
                response.headers_mut().insert(header_name, header_value);
            }
        }
    }

    Ok(response)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_minute, 100);
        assert_eq!(config.burst_size, 20);
        assert!(config.limit_unauthenticated);
        assert_eq!(config.unauthenticated_limit, 10);
    }

    #[test]
    fn test_rate_limit_entry_consume() {
        let entry = RateLimitEntry::new(5, Duration::from_secs(60));

        // Should be able to consume 5 tokens
        for _ in 0..5 {
            assert!(entry.try_consume().is_ok());
        }

        // 6th should fail
        assert!(entry.try_consume().is_err());
    }

    #[test]
    fn test_rate_limit_status() {
        let status = RateLimitStatus {
            limit: 100,
            remaining: 50,
            reset_after: 60,
        };

        assert_eq!(status.limit, 100);
        assert_eq!(status.remaining, 50);
        assert_eq!(status.reset_after, 60);
    }

    #[tokio::test]
    async fn test_rate_limiter_ip_check() {
        let config = RateLimitConfig {
            unauthenticated_limit: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Should allow 5 requests
        for i in 0..5 {
            let result = limiter.check_ip("192.168.1.1").await;
            assert!(result.is_ok(), "Request {} should succeed", i + 1);
        }

        // 6th should fail
        let result = limiter.check_ip("192.168.1.1").await;
        assert!(result.is_err(), "6th request should fail");
    }

    #[tokio::test]
    async fn test_rate_limiter_api_key_check() {
        let config = RateLimitConfig {
            requests_per_minute: 10,
            burst_size: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Should allow 15 requests (10 + 5 burst)
        for i in 0..15 {
            let result = limiter.check_api_key(1).await;
            assert!(result.is_ok(), "Request {} should succeed", i + 1);
        }

        // 16th should fail
        let result = limiter.check_api_key(1).await;
        assert!(result.is_err(), "16th request should fail");
    }

    #[test]
    fn test_extract_client_ip() {
        let mut headers = HeaderMap::new();

        // Test X-Forwarded-For
        headers.insert("X-Forwarded-For", "192.168.1.1, 10.0.0.1".parse().unwrap());
        let ip = extract_client_ip(&headers);
        assert_eq!(ip, "192.168.1.1");

        // Test X-Real-IP
        headers.clear();
        headers.insert("X-Real-IP", "10.0.0.2".parse().unwrap());
        let ip = extract_client_ip(&headers);
        assert_eq!(ip, "10.0.0.2");

        // Test fallback
        headers.clear();
        let ip = extract_client_ip(&headers);
        assert_eq!(ip, "unknown");
    }
}