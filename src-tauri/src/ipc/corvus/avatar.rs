//! `avatar` domain — git-provider avatar resolution routed through the
//! in-process broker.
//!
//! This is the **first async handler** and the template for the credential/
//! async cluster. An `async fn` handler registers as `Kind::Async` (the macro
//! reads its `async`-ness); the generic `rpc` command awaits it **on the
//! runtime** — the network round-trip yields the thread instead of blocking a
//! pool thread. The handler resolves the repo's `GitProvider` from `&AppState`
//! (`provider_for_tab`, a brief sync lock that returns owned `Arc`s — no guard
//! held across the `.await`, so the future stays `Send`), then `.await`s the
//! provider's REST lookup. The keyring-OOP broker is only needed once the
//! backend actually splits into its own process.
//!
//! Everything is best-effort: any failure (no remote, no token, no match)
//! returns `None` and the frontend falls back to a generated initials avatar.

use corvus_git_provider_api::prelude::resolve_avatar;

use crate::error::AppError;
use crate::git_provider::helpers::provider_for_tab;
use crate::ipc::corvus;
use crate::AppState;

#[corvus::handler]
async fn resolve_avatar_for_email(
    state: &AppState,
    tab_id: String,
    email: String,
) -> Result<Option<String>, AppError> {
    // No provider for this repo (local-only, Bitbucket, …) → quietly None.
    // `provider_for_tab` locks briefly and returns owned `Arc`s before we await.
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };
    Ok(resolve_avatar(resolved.provider.as_ref(), &resolved.info.remote_url, &email).await)
}
