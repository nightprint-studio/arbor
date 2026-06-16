//! Canonical entry point for `arbor-auth`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_auth::prelude::...` (or a single `use arbor_auth::prelude::*;`). The
//! submodules stay `pub` for rustdoc navigation but are not the canonical
//! call-site path.

pub use crate::error::{AuthError, Result};
pub use crate::oauth2::{DeviceFlow, InstalledAppFlow, PendingAuth, PollOutcome, refresh_token};
pub use crate::types::{BodyFormat, DeviceCode, TokenResponse};
