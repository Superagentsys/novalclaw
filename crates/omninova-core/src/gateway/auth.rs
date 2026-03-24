//! API Key Authentication Module
//!
//! Provides secure API key authentication for the HTTP Gateway.
//!
//! Features:
//! - API Key generation with SHA-256 hashing
//! - Permission-based access control (Read, Write, Admin)
//! - Key expiration support
//! - Usage tracking
//!
//! [Source: Story 8.3 - API 认证与授权]

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
    Extension,
};
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use thiserror::Error;

// ============================================================================
// Constants
// ============================================================================

/// API Key prefix for display (first N characters)
const KEY_PREFIX_LENGTH: usize = 8;

/// API Key length in bytes (32 bytes = 64 hex chars)
const KEY_LENGTH_BYTES: usize = 32;

// ============================================================================
// Error Types
// ============================================================================

/// Authentication error types
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Missing API key")]
    MissingApiKey,

    #[error("Invalid API key")]
    InvalidApiKey,

    #[error("API key has been revoked")]
    RevokedKey,

    #[error("API key has expired")]
    ExpiredKey,

    #[error("Insufficient permissions")]
    InsufficientPermissions,

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Invalid header format")]
    InvalidHeaderFormat,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AuthError::MissingApiKey => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "Missing API key"),
            AuthError::InvalidApiKey => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "Invalid API key"),
            AuthError::RevokedKey => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "API key has been revoked"),
            AuthError::ExpiredKey => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "API key has expired"),
            AuthError::InsufficientPermissions => (StatusCode::FORBIDDEN, "FORBIDDEN", "Insufficient permissions"),
            AuthError::DatabaseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "Authentication error"),
            AuthError::InvalidHeaderFormat => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "Invalid header format"),
        };

        let body = Json(serde_json::json!({
            "success": false,
            "error": {
                "code": code,
                "message": message
            }
        }));

        (status, body).into_response()
    }
}

// ============================================================================
// API Key Permission
// ============================================================================

/// API Key permission levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyPermission {
    /// Read-only access - can view agents and sessions
    Read,
    /// Read-write access - can create/modify agents and send messages
    Write,
    /// Admin access - can manage API keys and system configuration
    Admin,
}

impl ApiKeyPermission {
    /// Check if this permission level satisfies the required permission
    pub fn satisfies(&self, required: &ApiKeyPermission) -> bool {
        match required {
            ApiKeyPermission::Read => {
                // Read is satisfied by any permission level
                true
            }
            ApiKeyPermission::Write => {
                // Write requires Write or Admin
                matches!(self, ApiKeyPermission::Write | ApiKeyPermission::Admin)
            }
            ApiKeyPermission::Admin => {
                // Admin requires Admin
                matches!(self, ApiKeyPermission::Admin)
            }
        }
    }
}

impl std::fmt::Display for ApiKeyPermission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiKeyPermission::Read => write!(f, "read"),
            ApiKeyPermission::Write => write!(f, "write"),
            ApiKeyPermission::Admin => write!(f, "admin"),
        }
    }
}

// ============================================================================
// API Key Model
// ============================================================================

/// API Key data model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: i64,
    /// SHA-256 hash of the key
    pub key_hash: String,
    /// First 8 characters for display
    pub key_prefix: String,
    /// Human-readable name
    pub name: String,
    /// Permissions granted to this key
    pub permissions: Vec<ApiKeyPermission>,
    /// Creation timestamp (Unix)
    pub created_at: i64,
    /// Expiration timestamp (Unix), None means no expiration
    pub expires_at: Option<i64>,
    /// Last successful use timestamp (Unix)
    pub last_used_at: Option<i64>,
    /// Whether the key has been revoked
    pub is_revoked: bool,
}

impl ApiKey {
    /// Check if the key is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = chrono::Utc::now().timestamp();
            now > expires_at
        } else {
            false
        }
    }

    /// Check if the key is valid (not revoked and not expired)
    pub fn is_valid(&self) -> bool {
        !self.is_revoked && !self.is_expired()
    }

    /// Check if the key has a specific permission
    pub fn has_permission(&self, required: &ApiKeyPermission) -> bool {
        self.permissions.iter().any(|p| p.satisfies(required))
    }
}

// ============================================================================
// Create/Update Types
// ============================================================================

/// Request to create a new API key
#[derive(Debug, Clone, Deserialize)]
pub struct CreateApiKeyRequest {
    /// Human-readable name for the key
    pub name: String,
    /// Permissions to grant
    pub permissions: Vec<ApiKeyPermission>,
    /// Expiration in days from now (None = no expiration)
    pub expires_in_days: Option<u32>,
}

/// Response after creating an API key (full key shown only once!)
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyCreated {
    pub id: i64,
    /// The full API key (shown only once!)
    pub key: String,
    pub key_prefix: String,
    pub name: String,
    pub permissions: Vec<ApiKeyPermission>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

/// API key info for listing (without sensitive data)
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub key_prefix: String,
    pub name: String,
    pub permissions: Vec<ApiKeyPermission>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub is_revoked: bool,
    pub is_expired: bool,
}

impl From<ApiKey> for ApiKeyInfo {
    fn from(key: ApiKey) -> Self {
        let is_expired = key.is_expired();
        Self {
            id: key.id,
            key_prefix: key.key_prefix,
            name: key.name,
            permissions: key.permissions,
            created_at: key.created_at,
            expires_at: key.expires_at,
            last_used_at: key.last_used_at,
            is_revoked: key.is_revoked,
            is_expired,
        }
    }
}

// ============================================================================
// API Key Store
// ============================================================================

/// Store for API keys
pub struct ApiKeyStore {
    /// Database connection pool
    pool: Arc<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>,
}

impl ApiKeyStore {
    /// Create a new API key store
    pub fn new(pool: Arc<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>) -> Self {
        Self { pool }
    }

    /// Generate a new API key
    pub fn generate_key() -> String {
        use rand::RngCore;
        let mut key_bytes = [0u8; KEY_LENGTH_BYTES];
        rand::thread_rng().fill_bytes(&mut key_bytes);
        hex::encode(key_bytes)
    }

    /// Hash an API key using SHA-256
    pub fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash = hasher.finalize();
        hex::encode(hash)
    }

    /// Get the prefix of an API key (first 8 characters)
    pub fn get_prefix(key: &str) -> String {
        key.chars().take(KEY_PREFIX_LENGTH).collect()
    }

    /// Create a new API key
    pub fn create(&self, request: CreateApiKeyRequest) -> Result<ApiKeyCreated, AuthError> {
        let conn = self.pool.get().map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        // Generate new key
        let key = Self::generate_key();
        let key_hash = Self::hash_key(&key);
        let key_prefix = Self::get_prefix(&key);

        // Calculate expiration
        let expires_at = request.expires_in_days.map(|days| {
            chrono::Utc::now().timestamp() + (days as i64 * 24 * 60 * 60)
        });

        let created_at = chrono::Utc::now().timestamp();
        let permissions_json = serde_json::to_string(&request.permissions)
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        conn.execute(
            "INSERT INTO api_keys (key_hash, key_prefix, name, permissions, created_at, expires_at, last_used_at, is_revoked)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 0)",
            params![key_hash, key_prefix, request.name, permissions_json, created_at, expires_at],
        )
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let id = conn.last_insert_rowid();

        Ok(ApiKeyCreated {
            id,
            key,
            key_prefix,
            name: request.name,
            permissions: request.permissions,
            created_at,
            expires_at,
        })
    }

    /// Find an API key by its hash
    pub fn find_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, AuthError> {
        let conn = self.pool.get().map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, key_hash, key_prefix, name, permissions, created_at, expires_at, last_used_at, is_revoked
                 FROM api_keys WHERE key_hash = ?1"
            )
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let result = stmt.query_row(params![key_hash], |row| self.row_to_api_key(row));

        match result {
            Ok(key) => Ok(Some(key)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AuthError::DatabaseError(e.to_string())),
        }
    }

    /// Find an API key by ID
    pub fn find_by_id(&self, id: i64) -> Result<Option<ApiKey>, AuthError> {
        let conn = self.pool.get().map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, key_hash, key_prefix, name, permissions, created_at, expires_at, last_used_at, is_revoked
                 FROM api_keys WHERE id = ?1"
            )
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let result = stmt.query_row(params![id], |row| self.row_to_api_key(row));

        match result {
            Ok(key) => Ok(Some(key)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AuthError::DatabaseError(e.to_string())),
        }
    }

    /// List all API keys (as info without sensitive data)
    pub fn list_all(&self) -> Result<Vec<ApiKeyInfo>, AuthError> {
        let conn = self.pool.get().map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, key_hash, key_prefix, name, permissions, created_at, expires_at, last_used_at, is_revoked
                 FROM api_keys ORDER BY created_at DESC"
            )
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let keys = stmt
            .query_map([], |row| self.row_to_api_key(row))
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        Ok(keys.into_iter().map(ApiKeyInfo::from).collect())
    }

    /// Revoke an API key
    pub fn revoke(&self, id: i64) -> Result<bool, AuthError> {
        let conn = self.pool.get().map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let rows_affected = conn
            .execute(
                "UPDATE api_keys SET is_revoked = 1 WHERE id = ?1 AND is_revoked = 0",
                params![id],
            )
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        Ok(rows_affected > 0)
    }

    /// Delete an API key permanently
    pub fn delete(&self, id: i64) -> Result<bool, AuthError> {
        let conn = self.pool.get().map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let rows_affected = conn
            .execute("DELETE FROM api_keys WHERE id = ?1", params![id])
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        Ok(rows_affected > 0)
    }

    /// Update last used timestamp
    pub fn update_last_used(&self, id: i64) -> Result<(), AuthError> {
        let conn = self.pool.get().map_err(|e| AuthError::DatabaseError(e.to_string()))?;
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "UPDATE api_keys SET last_used_at = ?1 WHERE id = ?2",
            params![now, id],
        )
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Convert a database row to an ApiKey
    fn row_to_api_key(&self, row: &Row<'_>) -> Result<ApiKey, rusqlite::Error> {
        let permissions_json: String = row.get(4)?;
        let permissions: Vec<ApiKeyPermission> = serde_json::from_str(&permissions_json)
            .unwrap_or_else(|_| vec![ApiKeyPermission::Read]);

        Ok(ApiKey {
            id: row.get(0)?,
            key_hash: row.get(1)?,
            key_prefix: row.get(2)?,
            name: row.get(3)?,
            permissions,
            created_at: row.get(5)?,
            expires_at: row.get(6)?,
            last_used_at: row.get(7)?,
            is_revoked: row.get::<_, i32>(8)? != 0,
        })
    }
}

// ============================================================================
// Authentication Context
// ============================================================================

/// Authentication context extracted from a request
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub api_key_id: i64,
    pub key_name: String,
    pub permissions: Vec<ApiKeyPermission>,
}

impl AuthContext {
    /// Check if the context has a specific permission
    pub fn has_permission(&self, required: &ApiKeyPermission) -> bool {
        self.permissions.iter().any(|p| p.satisfies(required))
    }

    /// Require a specific permission, returning an error if not present
    pub fn require_permission(&self, required: &ApiKeyPermission) -> Result<(), AuthError> {
        if self.has_permission(required) {
            Ok(())
        } else {
            Err(AuthError::InsufficientPermissions)
        }
    }
}

// ============================================================================
// Authentication Middleware
// ============================================================================

/// Extract and validate API key from request headers
pub async fn extract_api_key(
    headers: &axum::http::HeaderMap,
    store: &ApiKeyStore,
) -> Result<AuthContext, AuthError> {
    // Try Bearer Token first
    if let Some(auth_header) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(bearer_token) = auth_str.strip_prefix("Bearer ") {
                return validate_key(bearer_token, store).await;
            }
        }
    }

    // Try X-API-Key header
    if let Some(api_key_header) = headers.get("X-API-Key") {
        if let Ok(api_key) = api_key_header.to_str() {
            return validate_key(api_key, store).await;
        }
    }

    Err(AuthError::MissingApiKey)
}

/// Validate an API key against the store
pub async fn validate_key(key: &str, store: &ApiKeyStore) -> Result<AuthContext, AuthError> {
    // Hash the key
    let key_hash = ApiKeyStore::hash_key(key);

    // Look up the key
    let api_key = store
        .find_by_hash(&key_hash)?
        .ok_or(AuthError::InvalidApiKey)?;

    // Check if revoked
    if api_key.is_revoked {
        return Err(AuthError::RevokedKey);
    }

    // Check if expired
    if api_key.is_expired() {
        return Err(AuthError::ExpiredKey);
    }

    // Update last used time
    let _ = store.update_last_used(api_key.id);

    Ok(AuthContext {
        api_key_id: api_key.id,
        key_name: api_key.name,
        permissions: api_key.permissions,
    })
}

/// Axum middleware for API key authentication
///
/// This middleware extracts and validates API keys from incoming requests.
/// On success, it injects `AuthContext` into request extensions.
/// On failure, it returns appropriate error responses (401/403).
///
/// # Example
///
/// ```ignore
/// use axum::Router;
/// use crate::gateway::auth::{auth_middleware, ApiKeyStore};
///
/// let api_key_store = Arc::new(ApiKeyStore::new(pool));
///
/// let app = Router::new()
///     .route("/api/protected", get(protected_handler))
///     .layer(axum::middleware::from_fn_with_state(
///         api_key_store.clone(),
///         auth_middleware,
///     ));
/// ```
pub async fn auth_middleware(
    State(store): State<Arc<ApiKeyStore>>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let headers = request.headers().clone();
    let auth_context = extract_api_key(&headers, &store).await?;

    // Inject auth context into request extensions
    request.extensions_mut().insert(auth_context);

    Ok(next.run(request).await)
}

/// Require a specific permission level for a request
///
/// Use this as an additional guard after the auth middleware.
///
/// # Example
///
/// ```ignore
/// async fn admin_handler(
///     Extension(auth): Extension<AuthContext>,
/// ) -> Result<Json<...>, AuthError> {
///     require_permission(&auth, &ApiKeyPermission::Admin)?;
///     // ... handle admin action
/// }
/// ```
pub fn require_permission(auth: &AuthContext, required: &ApiKeyPermission) -> Result<(), AuthError> {
    if auth.has_permission(required) {
        Ok(())
    } else {
        Err(AuthError::InsufficientPermissions)
    }
}

/// Extension extractor for AuthContext
///
/// Use this in handlers to get the authenticated user context.
///
/// # Example
///
/// ```ignore
/// async fn protected_handler(
///     Extension(auth): Extension<AuthContext>,
/// ) -> Json<String> {
///     Json(format!("Hello, {}!", auth.key_name))
/// }
/// ```
pub use axum::Extension as AuthExtension;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_hierarchy() {
        // Read is satisfied by all
        assert!(ApiKeyPermission::Read.satisfies(&ApiKeyPermission::Read));
        assert!(ApiKeyPermission::Write.satisfies(&ApiKeyPermission::Read));
        assert!(ApiKeyPermission::Admin.satisfies(&ApiKeyPermission::Read));

        // Write is satisfied by Write and Admin
        assert!(!ApiKeyPermission::Read.satisfies(&ApiKeyPermission::Write));
        assert!(ApiKeyPermission::Write.satisfies(&ApiKeyPermission::Write));
        assert!(ApiKeyPermission::Admin.satisfies(&ApiKeyPermission::Write));

        // Admin is only satisfied by Admin
        assert!(!ApiKeyPermission::Read.satisfies(&ApiKeyPermission::Admin));
        assert!(!ApiKeyPermission::Write.satisfies(&ApiKeyPermission::Admin));
        assert!(ApiKeyPermission::Admin.satisfies(&ApiKeyPermission::Admin));
    }

    #[test]
    fn test_key_hashing() {
        let key = "test-key-123";
        let hash1 = ApiKeyStore::hash_key(key);
        let hash2 = ApiKeyStore::hash_key(key);

        // Same key should produce same hash
        assert_eq!(hash1, hash2);

        // Hash should be SHA-256 length (64 hex chars)
        assert_eq!(hash1.len(), 64);

        // Different keys should produce different hashes
        let hash3 = ApiKeyStore::hash_key("different-key");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_key_prefix() {
        let key = "abcdefghijklmnopqrstuvwxyz1234567890";
        let prefix = ApiKeyStore::get_prefix(key);
        assert_eq!(prefix.len(), KEY_PREFIX_LENGTH);
        assert_eq!(prefix, "abcdefgh");
    }

    #[test]
    fn test_key_generation() {
        let key1 = ApiKeyStore::generate_key();
        let key2 = ApiKeyStore::generate_key();

        // Keys should be different
        assert_ne!(key1, key2);

        // Keys should be hex-encoded 32 bytes (64 chars)
        assert_eq!(key1.len(), 64);
        assert!(key1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_api_key_is_valid() {
        let now = chrono::Utc::now().timestamp();
        let one_hour_ago = now - 3600;
        let one_hour_from_now = now + 3600;

        // Valid key
        let valid_key = ApiKey {
            id: 1,
            key_hash: "hash".to_string(),
            key_prefix: "prefix".to_string(),
            name: "test".to_string(),
            permissions: vec![ApiKeyPermission::Read],
            created_at: now,
            expires_at: None,
            last_used_at: None,
            is_revoked: false,
        };
        assert!(valid_key.is_valid());

        // Revoked key
        let mut revoked_key = valid_key.clone();
        revoked_key.is_revoked = true;
        assert!(!revoked_key.is_valid());

        // Expired key
        let mut expired_key = valid_key.clone();
        expired_key.expires_at = Some(one_hour_ago);
        assert!(!expired_key.is_valid());

        // Not yet expired
        let mut future_key = valid_key.clone();
        future_key.expires_at = Some(one_hour_from_now);
        assert!(future_key.is_valid());
    }

    #[test]
    fn test_serialization() {
        let permissions = vec![ApiKeyPermission::Read, ApiKeyPermission::Write];
        let json = serde_json::to_string(&permissions).unwrap();
        assert_eq!(json, r#"["read","write"]"#);

        let parsed: Vec<ApiKeyPermission> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, permissions);
    }
}