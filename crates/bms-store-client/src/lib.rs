//! Client SDK for the bms-store API.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, Stream, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

pub use bms_core as core;
pub use bms_store_domain as domain;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const FAST_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TOKEN_REFRESH_LEAD_MS: i64 = 60 * 60 * 1000;
const ASSUMED_TOKEN_LIFETIME_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, thiserror::Error)]
pub enum BmsStoreError {
    #[error("client setup failed: {0}")]
    Setup(String),
    #[error("unreachable: {0}")]
    Unreachable(String),
    #[error("authentication failed")]
    AuthFailed,
    #[error("server returned status {0}")]
    BadStatus(u16),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("timeout")]
    Timeout,
    #[error("websocket error: {0}")]
    WebSocket(String),
}

impl BmsStoreError {
    pub fn is_auth_failure(&self) -> bool {
        matches!(self, BmsStoreError::AuthFailed)
    }

    pub fn is_unreachable(&self) -> bool {
        matches!(self, BmsStoreError::Unreachable(_) | BmsStoreError::Timeout)
    }
}

#[derive(Clone, Debug)]
pub struct BmsStoreCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Clone)]
enum AuthMode {
    Credentials {
        credentials: Arc<BmsStoreCredentials>,
        state: Arc<RwLock<AuthState>>,
    },
    ApiKey(Arc<String>),
}

#[derive(Default)]
struct AuthState {
    token: Option<String>,
    expires_at_ms: Option<i64>,
}

/// HTTP/WebSocket client for one bms-store instance.
#[derive(Clone)]
pub struct BmsStoreClient {
    base_url: Arc<String>,
    auth: AuthMode,
    http: reqwest::Client,
}

impl std::fmt::Debug for BmsStoreClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BmsStoreClient")
            .field("base_url", &*self.base_url)
            .finish()
    }
}

impl BmsStoreClient {
    pub fn new(base_url: &str, credentials: BmsStoreCredentials) -> Result<Self, BmsStoreError> {
        Self::with_timeout(base_url, credentials, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(
        base_url: &str,
        credentials: BmsStoreCredentials,
        timeout: Duration,
    ) -> Result<Self, BmsStoreError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .danger_accept_invalid_certs(false)
            .build()
            .map_err(|error| BmsStoreError::Setup(error.to_string()))?;
        Ok(Self {
            base_url: Arc::new(normalize_base_url(base_url)),
            auth: AuthMode::Credentials {
                credentials: Arc::new(credentials),
                state: Arc::new(RwLock::new(AuthState::default())),
            },
            http,
        })
    }

    pub fn for_connect_test(
        base_url: &str,
        credentials: BmsStoreCredentials,
    ) -> Result<Self, BmsStoreError> {
        let http = reqwest::Client::builder()
            .timeout(FAST_CONNECT_TIMEOUT)
            .connect_timeout(FAST_CONNECT_TIMEOUT)
            .build()
            .map_err(|error| BmsStoreError::Setup(error.to_string()))?;
        Ok(Self {
            base_url: Arc::new(normalize_base_url(base_url)),
            auth: AuthMode::Credentials {
                credentials: Arc::new(credentials),
                state: Arc::new(RwLock::new(AuthState::default())),
            },
            http,
        })
    }

    pub fn with_api_key(base_url: &str, api_key: impl Into<String>) -> Result<Self, BmsStoreError> {
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .build()
            .map_err(|error| BmsStoreError::Setup(error.to_string()))?;
        Ok(Self {
            base_url: Arc::new(normalize_base_url(base_url)),
            auth: AuthMode::ApiKey(Arc::new(api_key.into())),
            http,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn invalidate_token(&self) {
        if let AuthMode::Credentials { state, .. } = &self.auth {
            let mut state = state.write().await;
            state.token = None;
            state.expires_at_ms = None;
        }
    }

    pub async fn health(&self) -> Result<HealthResponse, BmsStoreError> {
        let url = format!("{}/api/health", self.base_url);
        let resp = self.http.get(&url).send().await.map_err(map_reqwest_err)?;
        if !resp.status().is_success() {
            return Err(BmsStoreError::BadStatus(resp.status().as_u16()));
        }
        resp.json::<HealthResponse>()
            .await
            .map_err(|error| BmsStoreError::Decode(error.to_string()))
    }

    pub async fn system_info(&self) -> Result<SystemInfoResponse, BmsStoreError> {
        self.get("/api/system/info").await
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, BmsStoreError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.authenticated_get(&url).await?;
        parse_json(resp).await
    }

    pub async fn get_query<T, Q>(&self, path: &str, query: &Q) -> Result<T, BmsStoreError>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.get(&url).query(query);
        req = self.apply_auth(req).await?;
        let resp = req.send().await.map_err(map_reqwest_err)?;
        let resp = self
            .retry_get_query_on_unauthorized(resp, &url, query)
            .await?;
        parse_json(resp).await
    }

    pub async fn post<B, T>(&self, path: &str, body: &B) -> Result<T, BmsStoreError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.post(&url).json(body);
        req = self.apply_auth(req).await?;
        let resp = req.send().await.map_err(map_reqwest_err)?;
        parse_json(resp).await
    }

    pub async fn put<B, T>(&self, path: &str, body: &B) -> Result<T, BmsStoreError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.put(&url).json(body);
        req = self.apply_auth(req).await?;
        let resp = req.send().await.map_err(map_reqwest_err)?;
        parse_json(resp).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, BmsStoreError> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.delete(&url);
        req = self.apply_auth(req).await?;
        let resp = req.send().await.map_err(map_reqwest_err)?;
        parse_json(resp).await
    }

    pub async fn event_stream(
        &self,
        subscribe: SubscribeRequest,
    ) -> Result<impl Stream<Item = Result<WsEvent, BmsStoreError>>, BmsStoreError> {
        let token = self.websocket_token().await?;
        let ws_url = websocket_url(&self.base_url, &token);
        let (mut ws, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|error| BmsStoreError::WebSocket(error.to_string()))?;
        let subscribe_json = serde_json::to_string(&serde_json::json!({ "subscribe": subscribe }))
            .map_err(|error| BmsStoreError::Decode(error.to_string()))?;
        ws.send(Message::Text(subscribe_json))
            .await
            .map_err(|error| BmsStoreError::WebSocket(error.to_string()))?;

        Ok(ws.filter_map(|message| async {
            match message {
                Ok(Message::Text(text)) => Some(
                    serde_json::from_str::<WsEvent>(&text)
                        .map_err(|error| BmsStoreError::Decode(error.to_string())),
                ),
                Ok(Message::Close(_)) => None,
                Ok(_) => None,
                Err(error) => Some(Err(BmsStoreError::WebSocket(error.to_string()))),
            }
        }))
    }

    async fn authenticated_get(&self, url: &str) -> Result<reqwest::Response, BmsStoreError> {
        let mut req = self.http.get(url);
        req = self.apply_auth(req).await?;
        let resp = req.send().await.map_err(map_reqwest_err)?;
        self.retry_get_on_unauthorized(resp, url).await
    }

    async fn apply_auth(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, BmsStoreError> {
        match &self.auth {
            AuthMode::ApiKey(key) => Ok(req.header("x-api-key", key.as_str())),
            AuthMode::Credentials { .. } => {
                let token = self.ensure_token().await?;
                Ok(req.bearer_auth(token))
            }
        }
    }

    async fn websocket_token(&self) -> Result<String, BmsStoreError> {
        match &self.auth {
            AuthMode::ApiKey(key) => Ok(key.as_ref().clone()),
            AuthMode::Credentials { .. } => self.ensure_token().await,
        }
    }

    async fn ensure_token(&self) -> Result<String, BmsStoreError> {
        let AuthMode::Credentials { credentials, state } = &self.auth else {
            return Err(BmsStoreError::AuthFailed);
        };

        {
            let state = state.read().await;
            if let (Some(token), Some(exp)) = (&state.token, state.expires_at_ms) {
                if exp - now_ms() > TOKEN_REFRESH_LEAD_MS {
                    return Ok(token.clone());
                }
            }
        }

        let url = format!("{}/api/auth/login", self.base_url);
        let body = serde_json::json!({
            "username": credentials.username,
            "password": credentials.password,
        });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_err)?;
        if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
            return Err(BmsStoreError::AuthFailed);
        }
        if !resp.status().is_success() {
            return Err(BmsStoreError::BadStatus(resp.status().as_u16()));
        }
        let login: LoginResponse = resp
            .json()
            .await
            .map_err(|error| BmsStoreError::Decode(error.to_string()))?;
        let exp = now_ms() + ASSUMED_TOKEN_LIFETIME_MS;
        {
            let mut state = state.write().await;
            state.token = Some(login.token.clone());
            state.expires_at_ms = Some(exp);
        }
        Ok(login.token)
    }

    async fn retry_get_on_unauthorized(
        &self,
        resp: reqwest::Response,
        url: &str,
    ) -> Result<reqwest::Response, BmsStoreError> {
        if resp.status().as_u16() != 401 || matches!(self.auth, AuthMode::ApiKey(_)) {
            return Ok(resp);
        }
        self.invalidate_token().await;
        let token = self.ensure_token().await?;
        let resp = self
            .http
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(map_reqwest_err)?;
        if resp.status().as_u16() == 401 {
            return Err(BmsStoreError::AuthFailed);
        }
        Ok(resp)
    }

    async fn retry_get_query_on_unauthorized<Q>(
        &self,
        resp: reqwest::Response,
        url: &str,
        query: &Q,
    ) -> Result<reqwest::Response, BmsStoreError>
    where
        Q: Serialize + ?Sized,
    {
        if resp.status().as_u16() != 401 || matches!(self.auth, AuthMode::ApiKey(_)) {
            return Ok(resp);
        }
        self.invalidate_token().await;
        let token = self.ensure_token().await?;
        let resp = self
            .http
            .get(url)
            .query(query)
            .bearer_auth(token)
            .send()
            .await
            .map_err(map_reqwest_err)?;
        if resp.status().as_u16() == 401 {
            return Err(BmsStoreError::AuthFailed);
        }
        Ok(resp)
    }
}

#[derive(Serialize, Debug, Clone, Default)]
pub struct SubscribeRequest {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since_seq: Option<i64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WsEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub seq: Option<i64>,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LoginResponse {
    pub token: String,
    pub user: serde_json::Value,
}

#[derive(Deserialize, Debug, Clone)]
pub struct HealthResponse {
    pub status: String,
    #[serde(default)]
    pub components: Vec<ComponentHealth>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ComponentHealth {
    pub name: String,
    pub status: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SystemInfoResponse {
    pub version: String,
    pub point_count: usize,
    pub device_count: usize,
    pub scenario_name: String,
}

fn normalize_base_url(s: &str) -> String {
    s.trim_end_matches('/').to_string()
}

fn websocket_url(base_url: &str, token: &str) -> String {
    let scheme = if base_url.starts_with("https://") {
        "wss://"
    } else {
        "ws://"
    };
    let without_scheme = base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    format!("{scheme}{without_scheme}/api/ws?token={token}")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn map_reqwest_err(error: reqwest::Error) -> BmsStoreError {
    if error.is_timeout() {
        BmsStoreError::Timeout
    } else if error.is_connect() || error.is_request() {
        BmsStoreError::Unreachable(error.to_string())
    } else if error.is_decode() {
        BmsStoreError::Decode(error.to_string())
    } else {
        BmsStoreError::Unreachable(error.to_string())
    }
}

async fn parse_json<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, BmsStoreError> {
    if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
        return Err(BmsStoreError::AuthFailed);
    }
    if !resp.status().is_success() {
        return Err(BmsStoreError::BadStatus(resp.status().as_u16()));
    }
    resp.json::<T>()
        .await
        .map_err(|error| BmsStoreError::Decode(error.to_string()))
}
