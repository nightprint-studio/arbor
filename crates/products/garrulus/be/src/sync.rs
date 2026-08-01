//! `sync` domain — the sync button's whole backend.
//!
//! Two rules from `docs/garrulus-design.md` §4 are load-bearing here and are worth
//! stating where the code is:
//!
//! 1. **Nothing writes without a click.** The only thing the background is
//!    allowed to call is [`probe_state`], which `probe`s. Everything that changes
//!    bytes — locally or remotely — is a handler the user's click reached.
//! 2. **A conflict never enters a file.** `pull` hands back the remote side as its
//!    own file beside the note (`garrulus-sync` writes it and names it); this
//!    domain only surfaces the list and applies the user's choice. No merge marker
//!    is ever written, so the vault still opens in Obsidian mid-conflict.
//!
//! ## Why these handlers are `async fn`
//!
//! `SyncRemote`'s methods are async, so the handlers that drive them are too:
//! `#[arbor_rpc::handler]` registers an `async fn` as `Kind::Async` and
//! `Dispatcher::into_fn` awaits it on the backend's real runtime, from the serve
//! loop's own request thread. Nothing here is on a runtime worker, so nothing here
//! is landmine #1 of `docs/backend-architecture.md` — and both `SyncRemote`
//! implementations put their blocking work (git, the filesystem, and the
//! credential `host_call` inside a fetch or a push) on `spawn_blocking`, which is
//! the trait's stated contract.
//!
//! The remote is cloned out of the state with [`GarrulusState::remote`] and the
//! guard is gone before any future is driven. That is not a style point: a guard
//! held across one of these calls is held across a round trip to the shell's
//! credential broker — see rule 2 of `garrulus_core::state`'s locking discipline.
//! It never bought serialisation either (read guards are shared), so **two clicks
//! can still overlap** — the frontend disables the sync button for the duration,
//! which is where that belongs.

use std::path::Path;
use std::sync::{LazyLock, Mutex};

use garrulus_core::prelude::{
    hooks, ChangeBatch, Conflict, GarrulusState, PullOutcome, Revision, SyncState,
};
use serde::Serialize;
use serde_json::json;

use crate::note;
use crate::vault_io;

/// The conflicts the last pull produced.
///
/// Backend-local on purpose: they are a property of *this session's* last pull,
/// not of the vault (the vault's own record is the side files on disk), and they
/// die with the process exactly as they should. A `garrulus_pull` replaces the
/// list wholesale.
static LAST_CONFLICTS: LazyLock<Mutex<Vec<Conflict>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// What a full sync did, for the button's toast and for the log.
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    /// Notes the pull brought in.
    pub applied: usize,
    /// Conflicts the pull could not merge — each already written beside its note.
    pub conflicts: usize,
    /// Whether the push half ran (it is skipped when the pull conflicted).
    pub pushed: bool,
}

/// The current sync state — the only thing the background ever asks.
///
/// Read-only by construction: `probe` fetches and reports, and cannot commit,
/// pull or push.
#[arbor_rpc::handler]
async fn garrulus_sync_state(state: &GarrulusState) -> Result<SyncState, String> {
    probe_state(state).await
}

/// Bring the remote's changes in.
#[arbor_rpc::handler]
async fn garrulus_pull(state: &GarrulusState) -> Result<PullOutcome, String> {
    let root = state.vault_root()?;
    state.fire_hook(hooks::SYNC_STARTED, json!({ "op": "pull" }));
    let outcome = pull_inner(state, &root).await?;
    fire_pull_hooks(state, &outcome);
    Ok(outcome)
}

/// Send the given notes in one batch, with an auto-generated message unless the
/// user wrote one. An **empty** list means every note the user has changed — see
/// [`ChangeBatch::is_empty`].
#[arbor_rpc::handler]
async fn garrulus_push(
    state: &GarrulusState,
    notes: Vec<String>,
    message: Option<String>,
) -> Result<(), String> {
    let root = state.vault_root()?;
    state.fire_hook(hooks::SYNC_STARTED, json!({ "op": "push", "notes": notes.len() }));
    let batch = ChangeBatch { notes: notes.into_iter().map(Into::into).collect(), message };
    push_inner(state, &root, &batch).await?;
    state.fire_hook(hooks::SYNC_DONE, json!({ "op": "push" }));
    Ok(())
}

/// The sync button's main action: pull, then push everything dirty.
///
/// The push is skipped when the pull conflicted — pushing on top of a conflict the
/// user has not looked at is how a vault ends up with two half-merged copies of a
/// note on two machines.
#[arbor_rpc::handler]
async fn garrulus_sync_now(
    state: &GarrulusState,
    message: Option<String>,
) -> Result<SyncReport, String> {
    let root = state.vault_root()?;
    state.fire_hook(hooks::SYNC_STARTED, json!({ "op": "sync" }));

    let outcome = pull_inner(state, &root).await?;
    fire_pull_hooks(state, &outcome);
    let report = SyncReport {
        applied:   outcome.applied.len(),
        conflicts: outcome.conflicts.len(),
        pushed:    outcome.conflicts.is_empty(),
    };
    if report.pushed {
        let batch = ChangeBatch { notes: Vec::new(), message };
        push_inner(state, &root, &batch).await?;
    }
    state.fire_hook(
        hooks::SYNC_DONE,
        json!({ "op": "sync", "applied": report.applied, "conflicts": report.conflicts }),
    );
    Ok(report)
}

/// The conflicts left by the last pull, for the Conflicts panel.
#[arbor_rpc::handler]
fn garrulus_conflicts(_state: &GarrulusState) -> Result<Vec<Conflict>, String> {
    Ok(LAST_CONFLICTS.lock().map_err(|_| "the conflict list is poisoned".to_string())?.clone())
}

/// Apply the user's choice to one conflict.
///
/// `resolution` is `"mine"` (keep the local note, drop the remote side file) or
/// `"theirs"` (the side file becomes the note). "Merge by hand" is not a
/// resolution: the user edits the note and then resolves as `"mine"`.
#[arbor_rpc::handler]
fn garrulus_resolve_conflict(
    state: &GarrulusState,
    path: String,
    side_file: String,
    resolution: String,
) -> Result<(), String> {
    let root = state.vault_root()?;
    let side = vault_io::resolve_rel(&root, &side_file)?;
    match resolution.as_str() {
        "mine" => {
            std::fs::remove_file(&side).map_err(|e| format!("{side_file}: {e}"))?;
        }
        "theirs" => {
            let note_path = vault_io::resolve_rel(&root, &path)?;
            std::fs::copy(&side, &note_path).map_err(|e| format!("{side_file} → {path}: {e}"))?;
            std::fs::remove_file(&side).map_err(|e| format!("{side_file}: {e}"))?;
            note::reindex(state, &path)?;
            state.fire_hook(hooks::NOTE_SAVED, json!({ "path": path, "source": "conflict" }));
        }
        other => return Err(format!("unknown resolution '{other}' (expected 'mine' or 'theirs')")),
    }
    forget_conflict(&path);
    Ok(())
}

/// The revisions of one note, newest first. Empty when the remote has no history
/// (`FolderRemote`) — the frontend hides the panel rather than showing a broken
/// one, which is what `RemoteCapabilities::history` is for.
#[arbor_rpc::handler]
async fn garrulus_note_history(
    state: &GarrulusState,
    path: String,
) -> Result<Vec<Revision>, String> {
    let root = state.vault_root()?;
    let remote = state.remote()?.ok_or_else(no_remote)?;
    remote.history(&root, &path.into()).await.map_err(|e| e.to_string())
}

/// One note's text as of a given revision, for the history panel's preview and
/// for "restore this version" (which writes through `garrulus_write_note`, so the
/// restore is an ordinary edit the user can undo).
///
/// `rev` is a [`Revision::id`] from [`garrulus_note_history`]; a remote without
/// history refuses both, which is what `RemoteCapabilities::history` warns about.
#[arbor_rpc::handler]
async fn garrulus_revision(
    state: &GarrulusState,
    path: String,
    rev: String,
) -> Result<String, String> {
    let root = state.vault_root()?;
    let remote = state.remote()?.ok_or_else(no_remote)?;
    remote.revision(&root, &path.into(), &rev).await.map_err(|e| e.to_string())
}

// ── Shared halves ─────────────────────────────────────────────────────────────

/// Ask the remote where the vault stands, without changing anything.
///
/// Shared by the handler and by the background probe, which must have no second
/// implementation of "what does read-only mean here" — one function, one set of
/// calls, and the guarantee holds for both callers or for neither.
pub(crate) async fn probe_state(state: &GarrulusState) -> Result<SyncState, String> {
    let Some(remote) = state.remote()? else {
        return Ok(SyncState::NoRemote);
    };
    remote.probe().await.map_err(|e| e.to_string())
}

/// Run the pull and refresh what it invalidated. Holds no lock while the remote
/// works, so the caller can fire hooks with nothing held.
async fn pull_inner(state: &GarrulusState, root: &Path) -> Result<PullOutcome, String> {
    let remote = state.remote()?.ok_or_else(no_remote)?;
    let outcome = remote.pull(root).await.map_err(|e| e.to_string())?;
    if !outcome.applied.is_empty() {
        // A pull rewrites an arbitrary set of files; rescanning is one call and a
        // pull is rare, so the index is rebuilt rather than patched note by note.
        let notes = vault_io::with_vault(state, vault_io::scan_notes)?;
        state.rebuild_index(notes)?;
    }
    remember_conflicts(&outcome.conflicts)?;
    Ok(outcome)
}

/// Run the push. Separate only so both callers share one "no destination" answer.
async fn push_inner(
    state: &GarrulusState,
    root: &Path,
    batch: &ChangeBatch,
) -> Result<(), String> {
    let remote = state.remote()?.ok_or_else(no_remote)?;
    remote.push(root, batch).await.map_err(|e| e.to_string())
}

/// Announce what the pull did, with every guard already dropped.
fn fire_pull_hooks(state: &GarrulusState, outcome: &PullOutcome) {
    if !outcome.conflicts.is_empty() {
        state.fire_hook(hooks::SYNC_CONFLICT, json!({ "count": outcome.conflicts.len() }));
    }
    state.fire_hook(
        hooks::SYNC_DONE,
        json!({
            "op":        "pull",
            "applied":   outcome.applied.len(),
            "conflicts": outcome.conflicts.len(),
        }),
    );
}

/// Replace the session's conflict list with the one this pull produced.
fn remember_conflicts(conflicts: &[Conflict]) -> Result<(), String> {
    let mut slot = LAST_CONFLICTS.lock().map_err(|_| "the conflict list is poisoned".to_string())?;
    *slot = conflicts.to_vec();
    Ok(())
}

/// Drop a resolved conflict from the session list.
///
/// Matched on the **wire form** of the conflict's path rather than on `RelPath`'s
/// concrete shape: that shape is `garrulus-sync`'s business, and the serialized
/// string is the form the frontend was given and is handing back. A failure to
/// match leaves a resolved entry in the list until the next pull, which is a
/// cosmetic problem, not a data one — hence no error path.
fn forget_conflict(path: &str) {
    let Ok(mut slot) = LAST_CONFLICTS.lock() else { return };
    slot.retain(|c| wire_path(&c.path).as_deref() != Some(path));
}

/// Drop the whole session list — the vault it describes is closing.
///
/// The list is process-global rather than state-owned (it is a property of *this
/// session's* last pull, not of the vault), which means nothing clears it on a
/// vault switch unless the close handler says so. Without this, closing a vault
/// with conflicts and opening another leaves the first vault's conflicts on
/// screen, pointing at paths the new vault does not have.
pub(crate) fn forget_all_conflicts() {
    let Ok(mut slot) = LAST_CONFLICTS.lock() else { return };
    slot.clear();
}

/// A conflict path as the frontend sees it, or `None` if it does not serialize to
/// a plain string.
fn wire_path<T: Serialize>(value: &T) -> Option<String> {
    match serde_json::to_value(value).ok()? {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    }
}

/// The one phrasing for "this vault has nowhere to sync to" — the sync button's
/// `no-remote` state, not a failure.
fn no_remote() -> String {
    "this vault has no sync destination configured".to_string()
}
