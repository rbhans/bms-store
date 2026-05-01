//! HTTP handlers for the Haystack op set. Each module owns one op.
//!
//! Implementation pattern:
//! 1. Negotiate `Accept`-based content type from headers + `?format=`.
//! 2. Decode request payload (Hayson grid or query params).
//! 3. Call into [`super::HaystackState`] for the actual data lookup.
//! 4. Serialize the resulting grid through [`super::ResponseBody`].

pub mod about;
pub mod defs;
pub mod filetypes;
pub mod his_read;
pub mod his_write;
pub mod invoke_action;
pub mod libs;
pub mod nav;
pub mod ops;
pub mod point_write;
pub mod read;
pub mod watch_poll;
pub mod watch_sub;
pub mod watch_unsub;

pub(crate) mod common;
