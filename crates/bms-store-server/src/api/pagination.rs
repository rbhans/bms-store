use serde::{Deserialize, Serialize};

use super::error::ApiError;

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 1000;

#[derive(Deserialize, Default)]
pub struct PaginationParams {
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    pub limit: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    pub offset: Option<i64>,
    pub cursor: Option<String>,
}

impl PaginationParams {
    /// Resolve limit and offset with defaults and clamping.
    pub fn resolve(&self) -> (i64, i64) {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let cursor_offset = self
            .cursor
            .as_deref()
            .and_then(|cursor| cursor.parse::<i64>().ok());
        let offset = self.offset.or(cursor_offset).unwrap_or(0).max(0);
        (limit, offset)
    }
}

#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl<T: Serialize> PaginatedResponse<T> {
    /// Build from a complete list, applying offset/limit.
    pub fn from_vec(all: Vec<T>, params: &PaginationParams) -> Self {
        let total = all.len() as i64;
        let (limit, offset) = params.resolve();
        let items: Vec<T> = all
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        let next_offset = offset + items.len() as i64;
        Self {
            items,
            total,
            limit,
            offset,
            next_cursor: (next_offset < total).then(|| next_offset.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Input validation
// ---------------------------------------------------------------------------

pub fn validate_string(name: &str, value: &str, max_len: usize) -> Result<(), ApiError> {
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{name} cannot be empty")));
    }
    if value.len() > max_len {
        return Err(ApiError::BadRequest(format!(
            "{name} exceeds maximum length of {max_len}"
        )));
    }
    Ok(())
}

pub fn validate_password(value: &str) -> Result<(), ApiError> {
    if value.len() < 8 {
        return Err(ApiError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    if value.len() > 256 {
        return Err(ApiError::BadRequest(
            "password exceeds maximum length of 256".into(),
        ));
    }
    Ok(())
}

fn deserialize_optional_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| serde::de::Error::custom("expected signed integer"))
            .map(Some),
        Some(serde_json::Value::String(value)) => value
            .parse::<i64>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected integer, got {other}"
        ))),
    }
}
