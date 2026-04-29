use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Unauthorized,
    Forbidden(String),
    Internal(String),
    TooManyRequests,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    code: u16,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
            Self::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many requests".to_string(),
            ),
        };
        let body = ErrorBody {
            error: message,
            code: status.as_u16(),
        };
        (status, axum::Json(body)).into_response()
    }
}

impl From<crate::store::node_store::NodeError> for ApiError {
    fn from(e: crate::store::node_store::NodeError) -> Self {
        match e {
            crate::store::node_store::NodeError::NotFound(_) => {
                Self::NotFound("node not found".into())
            }
            other => Self::Internal(other.to_string()),
        }
    }
}

impl From<crate::store::alarm_store::AlarmError> for ApiError {
    fn from(e: crate::store::alarm_store::AlarmError) -> Self {
        match e {
            crate::store::alarm_store::AlarmError::NotFound => {
                Self::NotFound("alarm not found".into())
            }
            other => Self::Internal(other.to_string()),
        }
    }
}

impl From<crate::store::schedule_store::ScheduleError> for ApiError {
    fn from(e: crate::store::schedule_store::ScheduleError) -> Self {
        match e {
            crate::store::schedule_store::ScheduleError::NotFound => {
                Self::NotFound("schedule not found".into())
            }
            other => Self::Internal(other.to_string()),
        }
    }
}

impl From<crate::store::user_store::UserError> for ApiError {
    fn from(e: crate::store::user_store::UserError) -> Self {
        match e {
            crate::store::user_store::UserError::NotFound => {
                Self::NotFound("user not found".into())
            }
            crate::store::user_store::UserError::UsernameExists => {
                Self::BadRequest("username already exists".into())
            }
            other => Self::Internal(other.to_string()),
        }
    }
}

impl From<crate::store::history_store::HistoryError> for ApiError {
    fn from(e: crate::store::history_store::HistoryError) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<crate::store::discovery_store::DiscoveryError> for ApiError {
    fn from(e: crate::store::discovery_store::DiscoveryError) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<crate::store::override_store::OverrideError> for ApiError {
    fn from(e: crate::store::override_store::OverrideError) -> Self {
        match e {
            crate::store::override_store::OverrideError::NotFound => {
                Self::NotFound("override not found".into())
            }
            other => Self::Internal(other.to_string()),
        }
    }
}
