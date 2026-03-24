//! API Request Logging Module
//!
//! Provides request logging and usage statistics for the HTTP Gateway.
//!
//! Features:
//! - Request/response logging with timing
//! - Query and filter capabilities
//! - Usage statistics aggregation
//! - Log retention and cleanup
//!
//! [Source: Story 8.4 - API 使用日志系统]

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Logging error types
#[derive(Debug, Error)]
pub enum LogError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Async error: {0}")]
    AsyncError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Invalid filter: {0}")]
    InvalidFilter(String),
}

impl IntoResponse for LogError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            LogError::DatabaseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
            LogError::AsyncError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Async error"),
            LogError::SerializationError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Serialization error"),
            LogError::InvalidFilter(_) => (StatusCode::BAD_REQUEST, "Invalid filter"),
        };

        let body = Json(serde_json::json!({
            "success": false,
            "error": {
                "code": "LOG_ERROR",
                "message": message
            }
        }));

        (status, body).into_response()
    }
}

// ============================================================================
// API Request Log Model
// ============================================================================

/// API Request Log record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequestLog {
    /// Unique identifier
    pub id: i64,
    /// Request timestamp (Unix)
    pub timestamp: i64,
    /// HTTP method (GET, POST, PUT, DELETE, etc.)
    pub method: String,
    /// Request endpoint path
    pub endpoint: String,
    /// HTTP status code
    pub status_code: u16,
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Associated API Key ID (if authenticated)
    pub api_key_id: Option<i64>,
    /// Client IP address
    pub ip_address: Option<String>,
    /// User-Agent string
    pub user_agent: Option<String>,
    /// Request body size in bytes
    pub request_size: Option<u64>,
    /// Response body size in bytes
    pub response_size: Option<u64>,
}

impl ApiRequestLog {
    /// Create a new log entry
    pub fn new(
        method: String,
        endpoint: String,
        status_code: u16,
        response_time_ms: u64,
    ) -> Self {
        Self {
            id: 0, // Will be set by database
            timestamp: chrono::Utc::now().timestamp(),
            method,
            endpoint,
            status_code,
            response_time_ms,
            api_key_id: None,
            ip_address: None,
            user_agent: None,
            request_size: None,
            response_size: None,
        }
    }

    /// Check if this is a successful request (2xx)
    pub fn is_success(&self) -> bool {
        self.status_code >= 200 && self.status_code < 300
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        self.status_code >= 400 && self.status_code < 500
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        self.status_code >= 500
    }

    /// Get the log level based on status code
    pub fn log_level(&self) -> &'static str {
        if self.is_success() {
            "INFO"
        } else if self.is_client_error() {
            "WARN"
        } else {
            "ERROR"
        }
    }
}

// ============================================================================
// Query Filter Types
// ============================================================================

/// Filter for querying API logs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestLogFilter {
    /// Start time (Unix timestamp)
    pub start_time: Option<i64>,
    /// End time (Unix timestamp)
    pub end_time: Option<i64>,
    /// Filter by endpoint (supports LIKE pattern)
    pub endpoint: Option<String>,
    /// Filter by HTTP method
    pub method: Option<String>,
    /// Filter by status code
    pub status_code: Option<u16>,
    /// Filter by API Key ID
    pub api_key_id: Option<i64>,
    /// Minimum response time (milliseconds)
    pub min_response_time: Option<u64>,
    /// Maximum response time (milliseconds)
    pub max_response_time: Option<u64>,
}

impl RequestLogFilter {
    /// Create a new filter for a time range
    pub fn time_range(start: i64, end: i64) -> Self {
        Self {
            start_time: Some(start),
            end_time: Some(end),
            ..Default::default()
        }
    }

    /// Create a filter for the last N hours
    pub fn last_hours(hours: i64) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            start_time: Some(now - hours * 3600),
            end_time: Some(now),
            ..Default::default()
        }
    }

    /// Create a filter for the last N days
    pub fn last_days(days: i64) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            start_time: Some(now - days * 24 * 3600),
            end_time: Some(now),
            ..Default::default()
        }
    }

    /// Filter by endpoint pattern
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Filter by method
    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }

    /// Filter by status code
    pub fn with_status_code(mut self, code: u16) -> Self {
        self.status_code = Some(code);
        self
    }
}

// ============================================================================
// Statistics Types
// ============================================================================

/// Time range for statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: i64,
    pub end: i64,
}

/// API usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUsageStats {
    /// Total number of requests
    pub total_requests: u64,
    /// Successful requests (2xx)
    pub successful_requests: u64,
    /// Client errors (4xx)
    pub client_errors: u64,
    /// Server errors (5xx)
    pub server_errors: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Maximum response time in milliseconds
    pub max_response_time_ms: u64,
    /// Minimum response time in milliseconds
    pub min_response_time_ms: u64,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Time range for these statistics
    pub time_range: TimeRange,
}

impl Default for ApiUsageStats {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            client_errors: 0,
            server_errors: 0,
            avg_response_time_ms: 0.0,
            max_response_time_ms: 0,
            min_response_time_ms: 0,
            error_rate: 0.0,
            time_range: TimeRange { start: 0, end: 0 },
        }
    }
}

/// Per-endpoint statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointStats {
    /// Endpoint path
    pub endpoint: String,
    /// HTTP method
    pub method: String,
    /// Number of requests
    pub request_count: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Number of errors (4xx + 5xx)
    pub error_count: u64,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
}

/// Per-API Key statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyStats {
    /// API Key ID
    pub api_key_id: i64,
    /// Key name (from api_keys table)
    pub key_name: Option<String>,
    /// Number of requests
    pub request_count: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Number of errors
    pub error_count: u64,
    /// Error rate
    pub error_rate: f64,
    /// Last request timestamp
    pub last_request_at: Option<i64>,
}

// ============================================================================
// API Log Store
// ============================================================================

/// Store for API request logs
pub struct ApiLogStore {
    /// Database connection pool
    pool: Arc<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>,
}

impl ApiLogStore {
    /// Create a new API log store
    pub fn new(pool: Arc<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>) -> Self {
        Self { pool }
    }

    /// Insert a new log entry
    pub fn insert(&self, log: &ApiRequestLog) -> Result<i64, LogError> {
        let conn = self.pool.get().map_err(|e| LogError::DatabaseError(e.to_string()))?;

        conn.execute(
            "INSERT INTO api_request_logs (timestamp, method, endpoint, status_code, response_time_ms, api_key_id, ip_address, user_agent, request_size, response_size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                log.timestamp,
                log.method,
                log.endpoint,
                log.status_code as i32,
                log.response_time_ms as i64,
                log.api_key_id,
                log.ip_address,
                log.user_agent,
                log.request_size.map(|s| s as i64),
                log.response_size.map(|s| s as i64),
            ],
        )
        .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        Ok(conn.last_insert_rowid())
    }

    /// Insert a log entry asynchronously (doesn't block)
    pub async fn insert_async(&self, log: ApiRequestLog) -> Result<i64, LogError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| LogError::DatabaseError(e.to_string()))?;

            conn.execute(
                "INSERT INTO api_request_logs (timestamp, method, endpoint, status_code, response_time_ms, api_key_id, ip_address, user_agent, request_size, response_size)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    log.timestamp,
                    log.method,
                    log.endpoint,
                    log.status_code as i32,
                    log.response_time_ms as i64,
                    log.api_key_id,
                    log.ip_address,
                    log.user_agent,
                    log.request_size.map(|s| s as i64),
                    log.response_size.map(|s| s as i64),
                ],
            )
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

            Ok(conn.last_insert_rowid())
        })
        .await
        .map_err(|e| LogError::AsyncError(e.to_string()))?
    }

    /// Query logs with filter
    pub fn query(
        &self,
        filter: &RequestLogFilter,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<ApiRequestLog>, LogError> {
        let conn = self.pool.get().map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, timestamp, method, endpoint, status_code, response_time_ms, api_key_id, ip_address, user_agent, request_size, response_size
             FROM api_request_logs WHERE 1=1"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(start) = filter.start_time {
            sql.push_str(" AND timestamp >= ?");
            params.push(Box::new(start));
        }
        if let Some(end) = filter.end_time {
            sql.push_str(" AND timestamp <= ?");
            params.push(Box::new(end));
        }
        if let Some(ref endpoint) = filter.endpoint {
            sql.push_str(" AND endpoint LIKE ?");
            params.push(Box::new(format!("%{}%", endpoint)));
        }
        if let Some(ref method) = filter.method {
            sql.push_str(" AND method = ?");
            params.push(Box::new(method.to_uppercase()));
        }
        if let Some(status) = filter.status_code {
            sql.push_str(" AND status_code = ?");
            params.push(Box::new(status as i32));
        }
        if let Some(key_id) = filter.api_key_id {
            sql.push_str(" AND api_key_id = ?");
            params.push(Box::new(key_id));
        }
        if let Some(min_time) = filter.min_response_time {
            sql.push_str(" AND response_time_ms >= ?");
            params.push(Box::new(min_time as i64));
        }
        if let Some(max_time) = filter.max_response_time {
            sql.push_str(" AND response_time_ms <= ?");
            params.push(Box::new(max_time as i64));
        }

        sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");
        params.push(Box::new(limit as i64));
        params.push(Box::new(offset as i64));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let logs = stmt
            .query_map(params_refs.as_slice(), |row| self.row_to_log(row))
            .map_err(|e| LogError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        Ok(logs)
    }

    /// Count logs matching filter
    pub fn count(&self, filter: &RequestLogFilter) -> Result<u64, LogError> {
        let conn = self.pool.get().map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let mut sql = String::from("SELECT COUNT(*) FROM api_request_logs WHERE 1=1");
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(start) = filter.start_time {
            sql.push_str(" AND timestamp >= ?");
            params.push(Box::new(start));
        }
        if let Some(end) = filter.end_time {
            sql.push_str(" AND timestamp <= ?");
            params.push(Box::new(end));
        }
        if let Some(ref endpoint) = filter.endpoint {
            sql.push_str(" AND endpoint LIKE ?");
            params.push(Box::new(format!("%{}%", endpoint)));
        }
        if let Some(ref method) = filter.method {
            sql.push_str(" AND method = ?");
            params.push(Box::new(method.to_uppercase()));
        }
        if let Some(status) = filter.status_code {
            sql.push_str(" AND status_code = ?");
            params.push(Box::new(status as i32));
        }
        if let Some(key_id) = filter.api_key_id {
            sql.push_str(" AND api_key_id = ?");
            params.push(Box::new(key_id));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let count: i64 = conn
            .query_row(&sql, params_refs.as_slice(), |row| row.get(0))
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        Ok(count as u64)
    }

    /// Get usage statistics for a time range
    pub fn get_stats(&self, start_time: i64, end_time: i64) -> Result<ApiUsageStats, LogError> {
        let conn = self.pool.get().map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let stats: Option<(i64, i64, i64, i64, f64, i64, i64)> = conn
            .query_row(
                "SELECT
                    COUNT(*) as total,
                    SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END) as success,
                    SUM(CASE WHEN status_code >= 400 AND status_code < 500 THEN 1 ELSE 0 END) as client_err,
                    SUM(CASE WHEN status_code >= 500 THEN 1 ELSE 0 END) as server_err,
                    AVG(response_time_ms) as avg_time,
                    MAX(response_time_ms) as max_time,
                    MIN(response_time_ms) as min_time
                 FROM api_request_logs
                 WHERE timestamp >= ?1 AND timestamp <= ?2",
                params![start_time, end_time],
                |row| Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?
                )),
            )
            .optional()
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        match stats {
            Some((total, success, client_err, server_err, avg_time, max_time, min_time)) if total > 0 => {
                let error_rate = (client_err + server_err) as f64 / total as f64;
                Ok(ApiUsageStats {
                    total_requests: total as u64,
                    successful_requests: success as u64,
                    client_errors: client_err as u64,
                    server_errors: server_err as u64,
                    avg_response_time_ms: avg_time,
                    max_response_time_ms: max_time as u64,
                    min_response_time_ms: min_time as u64,
                    error_rate,
                    time_range: TimeRange { start: start_time, end: end_time },
                })
            }
            _ => Ok(ApiUsageStats {
                time_range: TimeRange { start: start_time, end: end_time },
                ..Default::default()
            }),
        }
    }

    /// Get per-endpoint statistics for a time range
    pub fn get_endpoint_stats(&self, start_time: i64, end_time: i64, limit: u64) -> Result<Vec<EndpointStats>, LogError> {
        let conn = self.pool.get().map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT
                    endpoint,
                    method,
                    COUNT(*) as request_count,
                    AVG(response_time_ms) as avg_time,
                    SUM(CASE WHEN status_code >= 400 THEN 1 ELSE 0 END) as error_count
                 FROM api_request_logs
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 GROUP BY endpoint, method
                 ORDER BY request_count DESC
                 LIMIT ?3",
            )
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let stats = stmt
            .query_map(params![start_time, end_time, limit as i64], |row| {
                let request_count: i64 = row.get(2)?;
                let error_count: i64 = row.get(4)?;
                let error_rate = if request_count > 0 {
                    error_count as f64 / request_count as f64
                } else {
                    0.0
                };

                Ok(EndpointStats {
                    endpoint: row.get(0)?,
                    method: row.get(1)?,
                    request_count: request_count as u64,
                    avg_response_time_ms: row.get(3)?,
                    error_count: error_count as u64,
                    error_rate,
                })
            })
            .map_err(|e| LogError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        Ok(stats)
    }

    /// Get per-API Key statistics for a time range
    pub fn get_api_key_stats(&self, start_time: i64, end_time: i64, limit: u64) -> Result<Vec<ApiKeyStats>, LogError> {
        let conn = self.pool.get().map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT
                    l.api_key_id,
                    k.name as key_name,
                    COUNT(*) as request_count,
                    AVG(l.response_time_ms) as avg_time,
                    SUM(CASE WHEN l.status_code >= 400 THEN 1 ELSE 0 END) as error_count,
                    MAX(l.timestamp) as last_request_at
                 FROM api_request_logs l
                 LEFT JOIN api_keys k ON l.api_key_id = k.id
                 WHERE l.timestamp >= ?1 AND l.timestamp <= ?2 AND l.api_key_id IS NOT NULL
                 GROUP BY l.api_key_id
                 ORDER BY request_count DESC
                 LIMIT ?3",
            )
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let stats = stmt
            .query_map(params![start_time, end_time, limit as i64], |row| {
                let request_count: i64 = row.get(2)?;
                let error_count: i64 = row.get(4)?;
                let error_rate = if request_count > 0 {
                    error_count as f64 / request_count as f64
                } else {
                    0.0
                };

                Ok(ApiKeyStats {
                    api_key_id: row.get(0)?,
                    key_name: row.get(1)?,
                    request_count: request_count as u64,
                    avg_response_time_ms: row.get(3)?,
                    error_count: error_count as u64,
                    error_rate,
                    last_request_at: row.get(5)?,
                })
            })
            .map_err(|e| LogError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        Ok(stats)
    }

    /// Clear logs before a given timestamp
    pub fn clear_before(&self, timestamp: i64) -> Result<u64, LogError> {
        let conn = self.pool.get().map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let rows_deleted = conn
            .execute("DELETE FROM api_request_logs WHERE timestamp < ?1", params![timestamp])
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        Ok(rows_deleted as u64)
    }

    /// Clear all logs
    pub fn clear_all(&self) -> Result<u64, LogError> {
        let conn = self.pool.get().map_err(|e| LogError::DatabaseError(e.to_string()))?;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM api_request_logs", [], |row| row.get(0))
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        conn.execute("DELETE FROM api_request_logs", [])
            .map_err(|e| LogError::DatabaseError(e.to_string()))?;

        Ok(count as u64)
    }

    /// Export logs as JSON
    pub fn export_json(&self, filter: &RequestLogFilter) -> Result<String, LogError> {
        let logs = self.query(filter, 10000, 0)?; // Limit to 10k for export
        serde_json::to_string_pretty(&logs)
            .map_err(|e| LogError::SerializationError(e.to_string()))
    }

    /// Export logs as CSV
    pub fn export_csv(&self, filter: &RequestLogFilter) -> Result<String, LogError> {
        let logs = self.query(filter, 10000, 0)?; // Limit to 10k for export

        let mut csv = String::from("id,timestamp,method,endpoint,status_code,response_time_ms,api_key_id,ip_address,user_agent\n");

        for log in logs {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                log.id,
                log.timestamp,
                log.method,
                log.endpoint,
                log.status_code,
                log.response_time_ms,
                log.api_key_id.map(|id| id.to_string()).unwrap_or_default(),
                log.ip_address.unwrap_or_default(),
                log.user_agent.unwrap_or_default(),
            ));
        }

        Ok(csv)
    }

    /// Convert a database row to an ApiRequestLog
    fn row_to_log(&self, row: &Row<'_>) -> Result<ApiRequestLog, rusqlite::Error> {
        Ok(ApiRequestLog {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            method: row.get(2)?,
            endpoint: row.get(3)?,
            status_code: row.get::<_, i32>(4)? as u16,
            response_time_ms: row.get::<_, i64>(5)? as u64,
            api_key_id: row.get(6)?,
            ip_address: row.get(7)?,
            user_agent: row.get(8)?,
            request_size: row.get::<_, Option<i64>>(9)?.map(|s| s as u64),
            response_size: row.get::<_, Option<i64>>(10)?.map(|s| s as u64),
        })
    }
}

// ============================================================================
// Logging Middleware
// ============================================================================

/// Extract client IP address from headers
pub fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    // Try X-Forwarded-For first (for reverse proxy setups)
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(ip) = forwarded.to_str() {
            // X-Forwarded-For may contain multiple IPs, take the first one
            if let Some(client_ip) = ip.split(',').next() {
                return Some(client_ip.trim().to_string());
            }
        }
    }

    // Try X-Real-IP
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(ip) = real_ip.to_str() {
            return Some(ip.to_string());
        }
    }

    None
}

/// Extract User-Agent from headers
pub fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Axum middleware for request logging
///
/// This middleware records request metadata and timing information.
/// It runs after the auth middleware, so AuthContext is available in extensions.
///
/// # Example
///
/// ```ignore
/// use axum::Router;
/// use crate::gateway::logging::{logging_middleware, ApiLogStore};
///
/// let log_store = Arc::new(ApiLogStore::new(pool));
///
/// let app = Router::new()
///     .route("/api/agents", get(handler))
///     .layer(axum::middleware::from_fn_with_state(
///         log_store.clone(),
///         logging_middleware,
///     ));
/// ```
pub async fn logging_middleware(
    State(log_store): State<Arc<ApiLogStore>>,
    request: Request,
    next: Next,
) -> Response {
    let start = std::time::Instant::now();
    let method = request.method().to_string();
    let endpoint = request.uri().path().to_string();

    // Extract request metadata
    let headers = request.headers();
    let ip_address = extract_client_ip(headers);
    let user_agent = extract_user_agent(headers);
    let api_key_id = request
        .extensions()
        .get::<super::auth::AuthContext>()
        .map(|auth| auth.api_key_id);

    // Execute the request
    let response = next.run(request).await;

    // Calculate response time
    let response_time_ms = start.elapsed().as_millis() as u64;
    let status_code = response.status().as_u16();

    // Create log entry
    let log = ApiRequestLog {
        id: 0,
        timestamp: chrono::Utc::now().timestamp(),
        method,
        endpoint,
        status_code,
        response_time_ms,
        api_key_id,
        ip_address,
        user_agent,
        request_size: None,
        response_size: None,
    };

    // Log to tracing
    let level = log.log_level();
    if level == "ERROR" {
        tracing::error!(
            method = %log.method,
            endpoint = %log.endpoint,
            status = log.status_code,
            time_ms = log.response_time_ms,
            "API request"
        );
    } else if level == "WARN" {
        tracing::warn!(
            method = %log.method,
            endpoint = %log.endpoint,
            status = log.status_code,
            time_ms = log.response_time_ms,
            "API request"
        );
    } else {
        tracing::info!(
            method = %log.method,
            endpoint = %log.endpoint,
            status = log.status_code,
            time_ms = log.response_time_ms,
            "API request"
        );
    }

    // Async write to database (don't block the response)
    let store = log_store.clone();
    tokio::spawn(async move {
        if let Err(e) = store.insert_async(log).await {
            tracing::error!("Failed to insert API log: {}", e);
        }
    });

    response
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_log_creation() {
        let log = ApiRequestLog::new(
            "GET".to_string(),
            "/api/agents".to_string(),
            200,
            150,
        );

        assert_eq!(log.method, "GET");
        assert_eq!(log.endpoint, "/api/agents");
        assert_eq!(log.status_code, 200);
        assert_eq!(log.response_time_ms, 150);
        assert!(log.is_success());
        assert!(!log.is_client_error());
        assert!(!log.is_server_error());
        assert_eq!(log.log_level(), "INFO");
    }

    #[test]
    fn test_request_log_status_classification() {
        // Success (2xx)
        let success_log = ApiRequestLog::new("GET".into(), "/test".into(), 200, 10);
        assert!(success_log.is_success());
        assert_eq!(success_log.log_level(), "INFO");

        // Client error (4xx)
        let client_error_log = ApiRequestLog::new("GET".into(), "/test".into(), 404, 5);
        assert!(client_error_log.is_client_error());
        assert_eq!(client_error_log.log_level(), "WARN");

        // Server error (5xx)
        let server_error_log = ApiRequestLog::new("GET".into(), "/test".into(), 500, 100);
        assert!(server_error_log.is_server_error());
        assert_eq!(server_error_log.log_level(), "ERROR");
    }

    #[test]
    fn test_filter_time_range() {
        let now = chrono::Utc::now().timestamp();
        let filter = RequestLogFilter::time_range(now - 3600, now);

        assert_eq!(filter.start_time, Some(now - 3600));
        assert_eq!(filter.end_time, Some(now));
    }

    #[test]
    fn test_filter_last_hours() {
        let filter = RequestLogFilter::last_hours(24);

        assert!(filter.start_time.is_some());
        assert!(filter.end_time.is_some());

        let duration = filter.end_time.unwrap() - filter.start_time.unwrap();
        assert_eq!(duration, 24 * 3600);
    }

    #[test]
    fn test_filter_last_days() {
        let filter = RequestLogFilter::last_days(7);

        assert!(filter.start_time.is_some());
        assert!(filter.end_time.is_some());

        let duration = filter.end_time.unwrap() - filter.start_time.unwrap();
        assert_eq!(duration, 7 * 24 * 3600);
    }

    #[test]
    fn test_filter_with_endpoint() {
        let filter = RequestLogFilter::default()
            .with_endpoint("/api/agents");

        assert_eq!(filter.endpoint, Some("/api/agents".to_string()));
    }

    #[test]
    fn test_filter_with_method() {
        let filter = RequestLogFilter::default()
            .with_method("POST");

        assert_eq!(filter.method, Some("POST".to_string()));
    }

    #[test]
    fn test_filter_with_status_code() {
        let filter = RequestLogFilter::default()
            .with_status_code(404);

        assert_eq!(filter.status_code, Some(404));
    }

    #[test]
    fn test_usage_stats_default() {
        let stats = ApiUsageStats::default();

        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.successful_requests, 0);
        assert_eq!(stats.client_errors, 0);
        assert_eq!(stats.server_errors, 0);
        assert_eq!(stats.avg_response_time_ms, 0.0);
        assert_eq!(stats.error_rate, 0.0);
    }

    #[test]
    fn test_serialization() {
        let log = ApiRequestLog::new(
            "GET".to_string(),
            "/api/agents".to_string(),
            200,
            150,
        );

        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains("\"method\":\"GET\""));
        assert!(json.contains("\"endpoint\":\"/api/agents\""));

        let parsed: ApiRequestLog = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.method, log.method);
        assert_eq!(parsed.endpoint, log.endpoint);
    }
}