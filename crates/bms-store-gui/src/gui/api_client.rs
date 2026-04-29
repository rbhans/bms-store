//! HTTP API client for the Dioxus web (WASM) build.
//!
//! When the GUI is compiled to WASM, it can't access SQLite stores directly.
//! Instead, it talks to a running API server over HTTP/WebSocket.
//!
//! This module provides `ApiClient` — a typed wrapper around the REST API
//! that mirrors the store interfaces used by desktop components.
//!
//! # Usage (future)
//!
//! Components will use a `DataProvider` abstraction:
//! - Desktop: reads from stores directly (current behavior)
//! - Web: reads via `ApiClient` HTTP calls
//!
//! The migration path is incremental — each component can be updated independently.

use serde::{Deserialize, Serialize};

/// Configuration for connecting to the API server.
#[derive(Debug, Clone)]
pub struct ApiClientConfig {
    /// Base URL of the API server (e.g., "http://localhost:8080").
    pub base_url: String,
}

impl Default for ApiClientConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(), // Same origin in WASM
        }
    }
}

/// HTTP client for the OpenCrate BMS REST API.
#[derive(Debug, Clone)]
pub struct ApiClient {
    pub config: ApiClientConfig,
    /// JWT token for authenticated requests.
    token: Option<String>,
}

impl ApiClient {
    pub fn new(config: ApiClientConfig) -> Self {
        Self {
            config,
            token: None,
        }
    }

    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    pub fn clear_token(&mut self) {
        self.token = None;
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api{}", self.config.base_url, path)
    }

    fn auth_header(&self) -> Option<String> {
        self.token.as_ref().map(|t| format!("Bearer {t}"))
    }
}

// ----------------------------------------------------------------
// Request/Response types matching the API
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: ApiUser,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiPoint {
    pub device_id: String,
    pub point_id: String,
    pub display_name: Option<String>,
    pub value: Option<serde_json::Value>,
    pub unit: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiAlarm {
    pub id: String,
    pub device_id: String,
    pub point_id: String,
    pub severity: String,
    pub message: String,
    pub timestamp_ms: i64,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WritePointRequest {
    pub value: serde_json::Value,
    pub priority: Option<u8>,
}

// ----------------------------------------------------------------
// API methods
// ----------------------------------------------------------------

/// Errors from API calls.
#[derive(Debug, Clone)]
pub enum ApiError {
    /// HTTP request failed.
    Network(String),
    /// Server returned an error status.
    Server { status: u16, message: String },
    /// Failed to parse response.
    Parse(String),
    /// Not authenticated.
    Unauthorized,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::Server { status, message } => write!(f, "Server error {status}: {message}"),
            Self::Parse(msg) => write!(f, "Parse error: {msg}"),
            Self::Unauthorized => write!(f, "Unauthorized"),
        }
    }
}

impl ApiClient {
    /// Login and store the JWT token.
    pub async fn login(&mut self, username: &str, password: &str) -> Result<ApiUser, ApiError> {
        let body = LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        };
        let resp: LoginResponse = self.post("/auth/login", &body).await?;
        self.token = Some(resp.token);
        Ok(resp.user)
    }

    /// Get the current user profile.
    pub async fn me(&self) -> Result<ApiUser, ApiError> {
        self.get("/auth/me").await
    }

    /// List all points.
    pub async fn list_points(&self) -> Result<Vec<ApiPoint>, ApiError> {
        self.get("/points").await
    }

    /// Write a value to a point.
    pub async fn write_point(
        &self,
        device_id: &str,
        point_id: &str,
        value: serde_json::Value,
        priority: Option<u8>,
    ) -> Result<(), ApiError> {
        let body = WritePointRequest { value, priority };
        let _: serde_json::Value = self
            .post(&format!("/points/{device_id}/{point_id}/write"), &body)
            .await?;
        Ok(())
    }

    /// Get active alarms.
    pub async fn active_alarms(&self) -> Result<Vec<ApiAlarm>, ApiError> {
        self.get("/alarms/active").await
    }

    /// Acknowledge an alarm.
    pub async fn acknowledge_alarm(&self, alarm_id: &str) -> Result<(), ApiError> {
        let _: serde_json::Value = self
            .post(&format!("/alarms/{alarm_id}/ack"), &serde_json::json!({}))
            .await?;
        Ok(())
    }

    /// Health check (no auth required).
    pub async fn health(&self) -> Result<serde_json::Value, ApiError> {
        self.get_no_auth("/health").await
    }
}

// ----------------------------------------------------------------
// HTTP helpers (use reqwest on native, gloo-net on WASM)
// ----------------------------------------------------------------

impl ApiClient {
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ApiError> {
        let url = self.url(path);
        let auth = self.auth_header().ok_or(ApiError::Unauthorized)?;

        #[cfg(not(target_arch = "wasm32"))]
        {
            let client = reqwest::Client::new();
            let resp = client
                .get(&url)
                .header("Authorization", &auth)
                .send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?;
            parse_response(resp).await
        }

        #[cfg(target_arch = "wasm32")]
        {
            use gloo_net::http::Request;
            let resp = Request::get(&url)
                .header("Authorization", &auth)
                .send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?;
            parse_gloo_response(resp).await
        }
    }

    async fn get_no_auth<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ApiError> {
        let url = self.url(path);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let client = reqwest::Client::new();
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?;
            parse_response(resp).await
        }

        #[cfg(target_arch = "wasm32")]
        {
            use gloo_net::http::Request;
            let resp = Request::get(&url)
                .send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?;
            parse_gloo_response(resp).await
        }
    }

    async fn post<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let url = self.url(path);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let client = reqwest::Client::new();
            let mut req = client.post(&url).json(body);
            if let Some(ref auth) = self.auth_header() {
                req = req.header("Authorization", auth);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?;
            parse_response(resp).await
        }

        #[cfg(target_arch = "wasm32")]
        {
            use gloo_net::http::Request;
            let json = serde_json::to_string(body).map_err(|e| ApiError::Parse(e.to_string()))?;
            let mut req = Request::post(&url)
                .header("Content-Type", "application/json")
                .body(json)
                .map_err(|e| ApiError::Network(e.to_string()))?;
            if let Some(ref auth) = self.auth_header() {
                req = Request::post(&url)
                    .header("Content-Type", "application/json")
                    .header("Authorization", auth)
                    .body(serde_json::to_string(body).unwrap())
                    .map_err(|e| ApiError::Network(e.to_string()))?;
            }
            let resp = req
                .send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?;
            parse_gloo_response(resp).await
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn parse_response<T: for<'de> Deserialize<'de>>(
    resp: reqwest::Response,
) -> Result<T, ApiError> {
    let status = resp.status().as_u16();
    if status == 401 {
        return Err(ApiError::Unauthorized);
    }
    if !resp.status().is_success() {
        let msg = resp.text().await.unwrap_or_default();
        return Err(ApiError::Server {
            status,
            message: msg,
        });
    }
    resp.json()
        .await
        .map_err(|e| ApiError::Parse(e.to_string()))
}

#[cfg(target_arch = "wasm32")]
async fn parse_gloo_response<T: for<'de> Deserialize<'de>>(
    resp: gloo_net::http::Response,
) -> Result<T, ApiError> {
    let status = resp.status();
    if status == 401 {
        return Err(ApiError::Unauthorized);
    }
    if !resp.ok() {
        let msg = resp.text().await.unwrap_or_default();
        return Err(ApiError::Server {
            status,
            message: msg,
        });
    }
    resp.json()
        .await
        .map_err(|e| ApiError::Parse(e.to_string()))
}
