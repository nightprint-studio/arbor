//! Provider-agnostic OAuth2 building blocks.
//!
//! Supported flows:
//!
//! * **Installed-app (loopback + PKCE)** — [`oauth2::InstalledAppFlow`]. The
//!   bread-and-butter Arbor flow: PKCE code verifier/challenge, a fixed-port
//!   loopback HTTP listener, a branded success/error page, code → token
//!   exchange. Provider differences (body format, scope, extra query params,
//!   `client_secret` opt-in) are captured in the struct config.
//!
//! * **Refresh** — [`oauth2::refresh_token`]. Pure HTTP function, no UI.
//!
//! * **Device flow (RFC 8628)** — [`oauth2::DeviceFlow`]. For headless /
//!   secondary-device authorisation (typical GitHub-style PAT bootstrap).
//!
//! ## What lives elsewhere
//!
//! Token storage, post-callback provider-specific steps (e.g. Jira's
//! `cloud_resource` fetch), and event/hook delivery are **the consumer's
//! responsibility**. The flows here return a [`TokenResponse`] (or
//! [`DeviceCode`]) and let the caller drive persistence + side effects.

pub mod error;
pub mod html;
pub mod oauth2;
pub mod pkce;
pub mod types;

pub use error::{AuthError, Result};
pub use types::{BodyFormat, DeviceCode, TokenResponse};
