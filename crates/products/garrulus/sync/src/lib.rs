//! `garrulus-sync` — the sync seam of the note vault.
//!
//! One trait, [`remote::SyncRemote`], spoken in the vocabulary of *reconcile two
//! versions of a folder of notes* rather than *run git*, plus two
//! implementations that keep the trait honest: [`git::GitRemote`] (the real one,
//! over `corvus-git`) and [`folder::FolderRemote`] (a plain mirror directory —
//! a USB stick, or a folder some cloud client already syncs).
//!
//! Three invariants the whole crate exists to uphold
//! (`docs/garrulus-design.md` §1, §4.2, §4.4):
//!
//! 1. **Nothing writes without a user click.** The background only ever calls
//!    [`remote::SyncRemote::probe`], which is read-only.
//! 2. **Never lose a keystroke.** Anything that does not auto-merge is kept as a
//!    *visible artefact*: the local text stays in the note, the remote text is
//!    written beside it as a conflict side file, and both are reported in
//!    [`remote::PullOutcome`].
//! 3. **A merge marker never reaches a `.md`.** The vault must still open in
//!    Obsidian mid-conflict.
//!
//! With one deliberate exception to (2), which is §4.4.4: the vault's own
//! `.arbor/garrulus/` metadata is merged **by rule** ([`metadata`]) rather than
//! reported, because a conflict in a settings file is noise the user cannot act
//! on. Nothing is lost there either — the value the rule did not take is in the
//! merge's history.
//!
//! ## Public API: use the [`prelude`]
//!
//! ### On `garrulus-vault`
//!
//! The dependency is declared (the seam is defined over a vault, and the daily
//! note / attachment folders are vault settings), but no type is referenced yet:
//! the remotes take the plain data they need — a vault root [`std::path::Path`],
//! a device name, an optional daily-note folder — so the engine is testable
//! against a temp directory with no vault loaded. Wire the vault config into
//! [`git::GitRemote::with_daily_folder`] at construction time in `garrulus-core`.

pub mod change;
pub mod conflict;
pub mod error;
pub mod files;
pub mod folder;
pub mod frontmatter;
pub mod git;
pub mod keyed;
pub mod merge;
pub mod metadata;
pub mod prelude;
pub mod remote;
pub mod state;

use crate::error::{SyncError, SyncResult};

/// Run blocking work (git subprocesses, libgit2, filesystem walks) off the async
/// runtime's worker pool.
///
/// Every `SyncRemote` method is `async` but every implementation is blocking to
/// the bone. Doing that work inline would park a runtime worker for the whole
/// duration of a network fetch — the exact landmine `docs/backend-architecture.md`
/// documents for the reverse channel, where a parked worker means the shell can
/// no longer answer the credential request the fetch is waiting on.
pub(crate) async fn run_blocking<T, F>(f: F) -> SyncResult<T>
where
    F: FnOnce() -> SyncResult<T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(r) => r,
        Err(e) => Err(SyncError::Task(e.to_string())),
    }
}
