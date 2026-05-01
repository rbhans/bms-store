use serde::{Deserialize, Serialize};

/// Haystack `Ref` — opaque entity identifier with optional display string.
///
/// The `id` is the canonical id (excluding the leading `@`). `dis` is the
/// optional human-readable display string carried alongside the ref in
/// Hayson and Zinc encoded forms.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ref {
    pub id: String,
    pub dis: Option<String>,
}

impl Ref {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            dis: None,
        }
    }

    pub fn with_dis(id: impl Into<String>, dis: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            dis: Some(dis.into()),
        }
    }

    /// Render as `@id` (without display).
    pub fn as_zinc(&self) -> String {
        format!("@{}", self.id)
    }
}
