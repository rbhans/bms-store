use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::api::error::ApiError;
use crate::auth;
use crate::store::user_store::UserRole;

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

#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub role: UserRole,
    pub created_ms: i64,
    pub last_used_ms: Option<i64>,
    pub disabled: bool,
}

#[derive(Debug, Clone)]
pub struct ApiKeyPrincipal {
    pub id: String,
    pub name: String,
    pub role: UserRole,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedApiKey {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub key: String,
    pub role: UserRole,
    pub created_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    #[serde(deserialize_with = "deserialize_user_role")]
    pub role: UserRole,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateApiKeyRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_user_role")]
    pub role: Option<UserRole>,
    pub disabled: Option<bool>,
}

#[derive(Debug)]
struct ApiKeyStoreInner {
    path: PathBuf,
    keys: Vec<StoredApiKey>,
}

#[derive(Debug)]
pub struct ApiKeyStore {
    inner: Mutex<ApiKeyStoreInner>,
}

impl ApiKeyStore {
    pub fn new(path: PathBuf) -> Self {
        let keys = std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str::<Vec<StoredApiKey>>(&data).ok())
            .unwrap_or_default();

        Self {
            inner: Mutex::new(ApiKeyStoreInner { path, keys }),
        }
    }

    pub async fn list(&self) -> Vec<ApiKeyInfo> {
        let inner = self.inner.lock().await;
        let mut keys: Vec<ApiKeyInfo> = inner.keys.iter().map(ApiKeyInfo::from).collect();
        keys.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
        keys
    }

    pub async fn create(&self, req: CreateApiKeyRequest) -> Result<CreatedApiKey, ApiError> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err(ApiError::BadRequest("api key name is required".into()));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let key = generate_key();
        let prefix = key.chars().take(16).collect::<String>();
        let key_hash =
            auth::hash_password(&key).map_err(|error| ApiError::Internal(error.to_string()))?;
        let created_ms = now_ms();

        let mut inner = self.inner.lock().await;
        inner.keys.push(StoredApiKey {
            id: id.clone(),
            name: name.to_string(),
            prefix: prefix.clone(),
            key_hash,
            role: req.role.clone(),
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
            role: req.role,
            created_ms,
        })
    }

    pub async fn update(&self, id: &str, req: UpdateApiKeyRequest) -> Result<ApiKeyInfo, ApiError> {
        let mut inner = self.inner.lock().await;
        let key = inner
            .keys
            .iter_mut()
            .find(|key| key.id == id)
            .ok_or_else(|| ApiError::NotFound("api key not found".into()))?;

        if let Some(name) = req.name {
            let name = name.trim();
            if name.is_empty() {
                return Err(ApiError::BadRequest("api key name is required".into()));
            }
            key.name = name.to_string();
        }
        if let Some(role) = req.role {
            key.role = role;
        }
        if let Some(disabled) = req.disabled {
            key.disabled = disabled;
        }

        let out = ApiKeyInfo::from(&*key);
        persist(&inner)?;
        Ok(out)
    }

    pub async fn delete(&self, id: &str) -> Result<(), ApiError> {
        let mut inner = self.inner.lock().await;
        let before = inner.keys.len();
        inner.keys.retain(|key| key.id != id);
        if inner.keys.len() == before {
            return Err(ApiError::NotFound("api key not found".into()));
        }
        persist(&inner)
    }

    pub async fn authenticate(&self, presented: &str) -> Result<ApiKeyPrincipal, ApiError> {
        if !presented.starts_with("bsk_") {
            return Err(ApiError::Unauthorized);
        }

        let mut inner = self.inner.lock().await;
        let now = now_ms();
        for key in &mut inner.keys {
            if key.disabled {
                continue;
            }
            let valid = auth::verify_password(presented, &key.key_hash)
                .map_err(|error| ApiError::Internal(error.to_string()))?;
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

        Err(ApiError::Unauthorized)
    }
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

fn persist(inner: &ApiKeyStoreInner) -> Result<(), ApiError> {
    if let Some(parent) = inner.path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ApiError::Internal(error.to_string()))?;
    }
    let data = serde_json::to_string_pretty(&inner.keys)
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    std::fs::write(&inner.path, data).map_err(|error| ApiError::Internal(error.to_string()))
}

fn generate_key() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let secret = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("bsk_{secret}")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn deserialize_user_role<'de, D>(deserializer: D) -> Result<UserRole, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    parse_user_role(&raw).map_err(serde::de::Error::custom)
}

fn deserialize_optional_user_role<'de, D>(deserializer: D) -> Result<Option<UserRole>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    raw.as_deref()
        .map(parse_user_role)
        .transpose()
        .map_err(serde::de::Error::custom)
}

fn parse_user_role(raw: &str) -> Result<UserRole, String> {
    match raw.to_ascii_lowercase().as_str() {
        "admin" => Ok(UserRole::Admin),
        "operator" => Ok(UserRole::Operator),
        "viewer" => Ok(UserRole::Viewer),
        _ => Err(format!("unknown role: {raw}")),
    }
}
