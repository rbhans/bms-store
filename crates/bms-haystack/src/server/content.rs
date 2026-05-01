use axum::body::Body;
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::codec::hayson::Hayson;
use crate::codec::Codec;
use crate::val::Grid;

/// Content type negotiated for a single response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Hayson,
    Zinc,
    Csv,
    PlainJson,
}

impl ContentType {
    pub fn mime(self) -> &'static str {
        match self {
            ContentType::Hayson => "application/vnd.haystack+json",
            ContentType::Zinc => "text/zinc",
            ContentType::Csv => "text/csv",
            ContentType::PlainJson => "application/json",
        }
    }

    pub fn from_headers(headers: &HeaderMap, query_format: Option<&str>) -> Self {
        if let Some(q) = query_format {
            return Self::from_token(q);
        }
        if let Some(accept) = headers.get(ACCEPT).and_then(|v| v.to_str().ok()) {
            for token in accept.split(',') {
                let head = token.split(';').next().unwrap_or("").trim();
                let ct = Self::from_token(head);
                if !matches!(ct, ContentType::Hayson) {
                    return ct;
                }
                if head == "application/vnd.haystack+json" {
                    return ContentType::Hayson;
                }
            }
        }
        ContentType::Hayson
    }

    fn from_token(s: &str) -> Self {
        match s {
            "application/vnd.haystack+json" => ContentType::Hayson,
            "text/zinc" | "zinc" => ContentType::Zinc,
            "text/csv" | "csv" => ContentType::Csv,
            "application/json" | "json" => ContentType::PlainJson,
            _ => ContentType::Hayson,
        }
    }
}

/// A grid response that serializes per the negotiated content type.
pub struct ResponseBody {
    pub status: StatusCode,
    pub content_type: ContentType,
    pub grid: Grid,
}

impl ResponseBody {
    pub fn ok(grid: Grid, ct: ContentType) -> Self {
        Self {
            status: StatusCode::OK,
            content_type: ct,
            grid,
        }
    }
}

impl IntoResponse for ResponseBody {
    fn into_response(self) -> Response {
        let bytes = match self.content_type {
            ContentType::Hayson | ContentType::PlainJson => Hayson::encode_grid(&self.grid),
            ContentType::Zinc => Err(crate::codec::CodecError::Encode(
                "zinc codec not yet implemented".into(),
            )),
            ContentType::Csv => Err(crate::codec::CodecError::Encode(
                "csv codec not yet implemented".into(),
            )),
        };
        match bytes {
            Ok(b) => Response::builder()
                .status(self.status)
                .header(CONTENT_TYPE, HeaderValue::from_static(self.content_type.mime()))
                .body(Body::from(b))
                .unwrap(),
            Err(e) => Response::builder()
                .status(StatusCode::NOT_ACCEPTABLE)
                .body(Body::from(format!("encoding error: {e}")))
                .unwrap(),
        }
    }
}

/// Serialize a Haystack error as a 1-row grid with `err` marker + message
/// (the project-haystack idiom).
pub fn error_grid(msg: impl Into<String>) -> Grid {
    use crate::val::Dict;
    let mut meta = Dict::default();
    meta.marker("err");
    meta.insert("dis", crate::val::Value::Str(msg.into()));
    Grid {
        meta,
        cols: vec![],
        rows: vec![],
    }
}
