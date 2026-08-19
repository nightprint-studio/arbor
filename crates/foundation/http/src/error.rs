//! What can go wrong on a connection, and which of those the peer should hear about.
//!
//! The split that matters here is between a **transport** failure (the socket died —
//! nobody is left to tell) and a **malformed request** (someone is still listening, and
//! they get a status code back). [`HttpError::status`] is where that second half is
//! decided, once, so the serve loop never invents a code at the call site.

use std::io;

/// A failure while accepting, reading or answering on a connection.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    /// The socket failed. There is no response to write — the peer is gone.
    #[error("io: {0}")]
    Io(#[from] io::Error),

    /// The request line was not `METHOD PATH VERSION`.
    #[error("malformed request line")]
    MalformedRequestLine,

    /// A header line had no `:`, or its name/value was not valid text.
    #[error("malformed header")]
    MalformedHeader,

    /// The request head exceeded the configured cap before the blank line arrived.
    /// A client that never sends `\r\n\r\n` would otherwise hold memory forever.
    #[error("request head too large (limit {limit} bytes)")]
    HeadTooLarge { limit: usize },

    /// `Content-Length` exceeded the configured cap.
    #[error("request body too large ({len} bytes, limit {limit})")]
    BodyTooLarge { len: usize, limit: usize },

    /// `Content-Length` was present but not a number.
    #[error("invalid Content-Length")]
    InvalidContentLength,

    /// `Transfer-Encoding: chunked` — deliberately unsupported, see [`crate::request`].
    #[error("chunked transfer encoding is not supported")]
    ChunkedUnsupported,
}

impl HttpError {
    /// The status to answer with, or `None` when the connection is unusable and the
    /// only honest move is to drop it.
    pub fn status(&self) -> Option<u16> {
        match self {
            HttpError::Io(_) => None,
            HttpError::MalformedRequestLine
            | HttpError::MalformedHeader
            | HttpError::InvalidContentLength => Some(400),
            HttpError::HeadTooLarge { .. } => Some(431),
            HttpError::BodyTooLarge { .. } => Some(413),
            HttpError::ChunkedUnsupported => Some(411),
        }
    }
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, HttpError>;
