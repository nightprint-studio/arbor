//! Linear issue tracker — thin shell shim over `corvus-issue-tracker-linear`.
//!
//! The GraphQL/HTTP logic lives in the crate (keyring-free, credentials injected
//! via `SessionProvider`). What stays shell-side is the keyring glue: token
//! validation/storage and the host-gated inline-image fetch. The issue
//! *operations* (search/get/transition/assign/comment/create) and the
//! auth-status read now flow through the trait in `corvus-be`, so their shell
//! wrappers are gone.

use std::sync::Arc;

use corvus_issues::prelude::{validate_token, IssueTracker, IssueUser, LINEAR_GQL};

use crate::auth::credential_store;
use crate::error::Result;
use crate::integrations::registry::{registry, to_app_error};

const KEYRING_HOST: &str = "linear.app";
const KEYRING_USER: &str = "api-key";

/// The registered Linear tracker (always present once the registry is built).
fn tracker() -> Arc<dyn IssueTracker> {
    registry().get("linear").expect("linear tracker is always registered")
}

// ── Token storage (keyring — stays shell-side) ────────────────────────────────

fn save_token(token: &str) -> Result<()> {
    credential_store::save(KEYRING_HOST, KEYRING_USER, token)
}

// ── Auth ──────────────────────────────────────────────────────────────────────

pub async fn validate_and_save_token(token: &str) -> Result<IssueUser> {
    let user = validate_token(token, LINEAR_GQL).await.map_err(to_app_error)?;
    save_token(token)?;
    Ok(user)
}

// ── Inline-image proxy (host-gated, stays shell-side) ─────────────────────────

pub async fn fetch_image_bytes(url: &str) -> Result<(Vec<u8>, Option<String>)> {
    tracker().fetch_image_bytes(url).await.map_err(to_app_error)
}
