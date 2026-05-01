//! API key store — manages persistent API keys for programmatic access.
//!
//! Keys are stored as bcrypt hashes in `api_keys.json` alongside the project data.
//! The plain-text secret is returned exactly once on creation; after that only the
//! prefix (first 16 chars) and the hash are kept.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::auth;
use crate::store::user_store::UserRole;

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ApiKeyError {
    #[error("invalid request: {0}")]
    BadRequest(String),
    #[error("not found")]
    NotFound,
    #[error("internal error: {0}")]
    Internal(String),
}

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredApiKey {
    id: String,
    name: String,
    prefix: String,
    key_hash: String,
    role: UserRole,
    created_ms: i64,
    last_used_ms: Option<i64>,
    disabled: bool,
}

/// Public metadata for a stored API key (no secret).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub role: UserRole,
    pub created_ms: i64,
    pub last_used_ms: Option<i64>,
    pub disabled: bool,
}

impl From<&StoredApiKey> for ApiKeyInfo {
    fn from(key: &StoredApiKey) -> Self {
        Self {
            id: key.id.clone(),
            name: key.name.clone(),
            prefix: key.prefix.clone(),
            role: key.role.clone(),
            created_ms: key.created_ms,
            last_used_ms: key.last_used_ms,
            disabled: key.disabled,
        }
    }
}

/// Returned exactly once — on key creation. The `key` field is the full secret.
#[derive(Debug, Clone, Serialize)]
pub struct CreatedApiKey {
    pub id: String,
    pub name: String,
    pub prefix: String,
    /// Full `bsk_…` secret — show to the user once; never stored.
    pub key: String,
    pub role: UserRole,
    pub created_ms: i64,
}

/// A verified principal identified by an API key.
#[derive(Debug, Clone)]
pub struct ApiKeyPrincipal {
    pub id: String,
    pub name: String,
    pub role: UserRole,
}

// ── Store ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Inner {
    path: PathBuf,
    keys: Vec<StoredApiKey>,
}

/// Thread-safe, file-backed API key store.
#[derive(Debug)]
pub struct ApiKeyStore {
    inner: Mutex<Inner>,
}

impl ApiKeyStore {
    /// Load (or create) the key store from `path`.
    pub fn new(path: PathBuf) -> Self {
        let keys = std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str::<Vec<StoredApiKey>>(&data).ok())
            .unwrap_or_default();
        Self {
            inner: Mutex::new(Inner { path, keys }),
        }
    }

    /// List all keys, most recent first.
    pub async fn list(&self) -> Vec<ApiKeyInfo> {
        let inner = self.inner.lock().await;
        let mut keys: Vec<ApiKeyInfo> = inner.keys.iter().map(ApiKeyInfo::from).collect();
        keys.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
        keys
    }

    /// Create a new API key. Returns the full secret exactly once.
    pub async fn create(&self, name: &str, role: UserRole) -> Result<CreatedApiKey, ApiKeyError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiKeyError::BadRequest("API key name is required".into()));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let key = generate_key();
        let prefix = key.chars().take(16).collect::<String>();
        let key_hash = auth::hash_password(&key)
            .map_err(|e| ApiKeyError::Internal(e.to_string()))?;
        let created_ms = now_ms();

        let mut inner = self.inner.lock().await;
        inner.keys.push(StoredApiKey {
            id: id.clone(),
            name: name.to_string(),
            prefix: prefix.clone(),
            key_hash,
            role: role.clone(),
            created_ms,
            last_used_ms: None,
            disabled: false,
        });
        persist(&inner)?;

        Ok(CreatedApiKey {
            id,
            name: name.to_string(),
            prefix,
            key,
            role,
            created_ms,
        })
    }

    /// Update name, role, or disabled flag for an existing key.
    pub async fn update(
        &self,
        id: &str,
        name: Option<String>,
        role: Option<UserRole>,
        disabled: Option<bool>,
    ) -> Result<ApiKeyInfo, ApiKeyError> {
        let mut inner = self.inner.lock().await;
        let key = inner
            .keys
            .iter_mut()
            .find(|k| k.id == id)
            .ok_or(ApiKeyError::NotFound)?;

        if let Some(n) = name {
            let n = n.trim().to_string();
            if n.is_empty() {
                return Err(ApiKeyError::BadRequest("API key name is required".into()));
            }
            key.name = n;
        }
        if let Some(r) = role {
            key.role = r;
        }
        if let Some(d) = disabled {
            key.disabled = d;
        }

        let out = ApiKeyInfo::from(&*key);
        persist(&inner)?;
        Ok(out)
    }

    /// Permanently delete a key.
    pub async fn delete(&self, id: &str) -> Result<(), ApiKeyError> {
        let mut inner = self.inner.lock().await;
        let before = inner.keys.len();
        inner.keys.retain(|k| k.id != id);
        if inner.keys.len() == before {
            return Err(ApiKeyError::NotFound);
        }
        persist(&inner)
    }

    /// Authenticate a presented secret. Updates `last_used_ms` on success.
    pub async fn authenticate(&self, presented: &str) -> Result<ApiKeyPrincipal, ApiKeyError> {
        if !presented.starts_with("bsk_") {
            return Err(ApiKeyError::BadRequest("invalid key format".into()));
        }

        let mut inner = self.inner.lock().await;
        let now = now_ms();
        for key in &mut inner.keys {
            if key.disabled {
                continue;
            }
            let valid = auth::verify_password(presented, &key.key_hash)
                .map_err(|e| ApiKeyError::Internal(e.to_string()))?;
            if valid {
                key.last_used_ms = Some(now);
                let principal = ApiKeyPrincipal {
                    id: key.id.clone(),
                    name: key.name.clone(),
                    role: key.role.clone(),
                };
                persist(&inner)?;
                return Ok(principal);
            }
        }
        Err(ApiKeyError::BadRequest("invalid or unknown key".into()))
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn persist(inner: &Inner) -> Result<(), ApiKeyError> {
    if let Some(parent) = inner.path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiKeyError::Internal(e.to_string()))?;
    }
    let data = serde_json::to_string_pretty(&inner.keys)
        .map_err(|e| ApiKeyError::Internal(e.to_string()))?;
    std::fs::write(&inner.path, data)
        .map_err(|e| ApiKeyError::Internal(e.to_string()))
}

fn generate_key() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let secret: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("bsk_{secret}")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
