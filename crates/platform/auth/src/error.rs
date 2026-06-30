use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    /// Loopback listener bind / accept / read failure, or any other low-level
    /// transport error.
    #[error("Transport: {0}")]
    Transport(String),

    /// The provider returned an error (e.g. user denied, expired code,
    /// invalid client_id). The string is the human-readable reason that
    /// also gets rendered into the error HTML.
    #[error("Provider error: {0}")]
    Provider(String),

    /// State mismatch on the callback — possible CSRF, treated as auth
    /// failure with a fixed user-facing message.
    #[error("State mismatch — authorisation rejected as a CSRF defence")]
    StateMismatch,

    /// HTTP exchange (token / refresh / device-code endpoints) failed at
    /// the request or response decode stage.
    #[error("HTTP: {0}")]
    Http(String),

    /// Token endpoint returned non-2xx. Body included verbatim.
    #[error("HTTP {status}: {body}")]
    HttpStatus { status: u16, body: String },

    /// Device-flow polling expired before the user authorised the code.
    #[error("Device-flow code expired before authorisation")]
    DeviceCodeExpired,
}

pub type Result<T> = std::result::Result<T, AuthError>;
