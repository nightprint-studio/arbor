//! `remote` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name** (reading the signature to generate the JSON-arg decode), so the
//! command is reached generically through the router. Behavior (locks held,
//! errors) is byte-identical — only the call path changed.
//!
//! The git work stays in the reusable shell module [`crate::git::remote`].
//! Only the **leaf, credential-free** remote-config query lives here:
//! `list_remotes` reads the repo's configured remotes off the shared handle,
//! no network I/O, no AppHandle, no emit.
//!
//! The network-coupled commands stay inline in the legacy command module for
//! the later credential wave (broker gate): `fetch_remote`, `push_branch`,
//! `pull_branch` all inject host-scoped HTTPS auth, run async on the blocking
//! pool, and (for pull) take an `AppHandle` to stream `arbor://pull-progress`
//! events. `open_in_browser` also stays inline — it takes an `AppHandle` and
//! drives the system opener.
//!
//! No hooks fire for `list_remotes` (the deferred network commands fire
//! `on_fetch` / `on_push` / `on_pull`).

use crate::error::AppError;
use crate::git::remote::RemoteInfo;
use crate::ipc::corvus;
use crate::AppState;

#[corvus::handler]
fn list_remotes(state: &AppState, tab_id: String) -> Result<Vec<RemoteInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::remote::list_remotes(repo.inner())
}
