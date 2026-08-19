//! Canonical entry point for `arbor-http`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_http::prelude::...`.

pub use crate::error::{HttpError, Result};
pub use crate::request::{percent_decode, Request};
pub use crate::response::{Body, Response, SseEvent};
pub use crate::server::{error_response, Server, ServerConfig};
