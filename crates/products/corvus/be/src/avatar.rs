//! `avatar` domain — git-provider avatar resolution, served **out-of-process**
//! by corvus-be.
//!
//! Same handler (function name → method name) as the shell's in-process copy
//! (`crate::ipc::corvus::avatar`), but the context is [`CorvusState`] and the
//! provider comes from the reverse-channel registry ([`crate::provider`]). The
//! REST lookup itself is the shared `GitProvider::avatar_url_for_email` (over the
//! provider's keyring-free HTTP layer), and the cache + machine-email skip are
//! the shared `corvus_git_provider_api::avatar::resolve_avatar` wrapper — so the
//! result is identical in- and out-of-process. Best-effort: any failure (no
//! remote, no token, no match) returns `None` and the FE falls back to a
//! generated initials avatar. No hooks fire in this domain.

use corvus_core::prelude::CorvusState;
use corvus_git_provider_api::prelude::resolve_avatar;

use crate::provider::provider_for_tab;

#[arbor_rpc::handler]
async fn resolve_avatar_for_email(
    state: &CorvusState,
    tab_id: String,
    email: String,
) -> Result<Option<String>, String> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r) => r,
        // No provider for this repo (local-only, Bitbucket, …) → quietly None.
        Err(_) => return Ok(None),
    };
    Ok(resolve_avatar(resolved.provider.as_ref(), &resolved.info.remote_url, &email).await)
}
