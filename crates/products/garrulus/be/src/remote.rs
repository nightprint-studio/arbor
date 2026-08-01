//! `remote` domain — where the open vault syncs to.
//!
//! Until something calls [`GarrulusState::set_remote`], every sync handler
//! returns `SyncState::NoRemote` by construction. This module is the only thing
//! that calls it, from four places: the user configuring a destination, the user
//! creating one, the vault being opened with one already recorded, and the user
//! clearing it.
//!
//! ## Where the configuration lives, and why it is not in the vault
//!
//! In the **profile registry** (`garrulus/vaults.json`, i.e. `VaultEntry.remote`),
//! never in `<vault>/.arbor/garrulus/vault.toml`. Two reasons, both load-bearing:
//!
//! * a folder mirror's path is machine-specific (a USB mount, a Drive folder) and
//!   would be *wrong* on the other PC the moment the vault synced there;
//! * a git remote's URL already lives in `.git/config`, so a second copy in the
//!   vault is a copy that can drift.
//!
//! This is the same rule `corvus/repos.json` follows: the registry is the one
//! place machine-specific absolute paths are allowed.
//!
//! ## Building the destination
//!
//! Never here. `garrulus_core::build_remote` is the one factory that turns a
//! [`RemoteConfig`] into a `GitRemote` or a `FolderRemote` — it owns the
//! credential bridge onto the shell and the device name. Every call site in this
//! file goes through it, so there is exactly one answer to "how is a remote
//! wired".

use std::path::Path;

// `GitCli` comes through the product's own prelude, which re-exports it for
// exactly this call site — taking a second `corvus-git` dependency here for one
// type would make that re-export dead weight.
use garrulus_core::prelude::{
    build_remote, load_vaults, set_vault_remote, vault_id_for, GarrulusState, GitCli, RemoteConfig,
    RemoteDescriptor, RemoteKind, SyncState, DEFAULT_GIT_REMOTE,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::probe;
use crate::sync;

/// Identity **and** standing of the configured destination, in one shape.
///
/// Returned as a unit so the title-bar sync button can be drawn from a single
/// round trip: it needs the descriptor (which icon, does this remote have
/// history) and the state (which colour, is there anything to do) together, and
/// asking for them separately would let the two disagree.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteStatus {
    /// What the destination is — id, kind, display name, capabilities.
    pub descriptor: RemoteDescriptor,
    /// Where the vault stands against it, as of one probe just now.
    pub state: SyncState,
}

/// What the shell hands back after creating a repository for us.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatedRepo {
    /// The URL to give `git remote add`.
    clone_url: String,
    /// The provider page, for the log line and the "open in browser" affordance.
    #[serde(default)]
    web_url: String,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// Point the open vault at a sync destination: persist it, install it, probe it.
///
/// Persisting *before* installing is deliberate. A destination the user
/// configured and that then failed to answer is still the destination they
/// configured — losing it because the network was down for one second would be
/// the worse failure, and the probe result already tells them it is unreachable.
#[arbor_rpc::handler]
async fn garrulus_set_remote(
    state: &GarrulusState,
    config: RemoteConfig,
) -> Result<RemoteStatus, String> {
    let root = state.vault_root()?;
    install_and_persist(state, &root, config).await
}

/// Make the open vault local-only again.
///
/// Clears the registry entry and detaches the live remote. The vault's own files
/// are untouched — a git vault keeps its `.git`, a mirrored vault keeps its
/// mirror; this is "stop syncing", not "undo the sync".
#[arbor_rpc::handler]
fn garrulus_clear_remote(state: &GarrulusState) -> Result<(), String> {
    let root = state.vault_root()?;
    persist(&root, None)?;
    state.clear_remote()?;
    probe::forget();
    Ok(())
}

/// The destination recorded for the open vault, if any.
///
/// Read from the registry rather than from the installed remote: the registry is
/// the record, and a remote that failed to build at open time must still show the
/// user what they had configured — otherwise the settings panel would come up
/// empty and invite them to type it in again.
#[arbor_rpc::handler]
fn garrulus_remote_config(state: &GarrulusState) -> Result<Option<RemoteConfig>, String> {
    let root = state.vault_root()?;
    Ok(stored(&root))
}

/// Try a destination without committing to it — the settings panel's "test"
/// button.
///
/// Builds and probes, and persists **nothing**: a configuration the user is still
/// typing must not be able to replace the one that works. Unlike
/// [`garrulus_set_remote`], a probe failure is returned as an error rather than
/// softened to `Offline` — "does this work?" is exactly the question being asked,
/// so the answer has to carry the reason.
#[arbor_rpc::handler]
async fn garrulus_test_remote(
    state: &GarrulusState,
    config: RemoteConfig,
) -> Result<RemoteStatus, String> {
    let root = state.vault_root()?;
    let remote = build_remote(state, &root, &config)?;
    let descriptor = remote.descriptor();
    let probed = remote.probe().await.map_err(|e| e.to_string())?;
    Ok(RemoteStatus { descriptor, state: probed })
}

/// Create a private repository through the shell's git provider and adopt it.
///
/// The provider trait, the OAuth tokens and the "which account" question all live
/// in the shell; this backend owns none of them and must not learn to. So the
/// creation is one reverse-channel call, and everything after it is plain git.
///
/// **The repository is always private.** There is no public option at any layer of
/// this flow — a personal note vault has no business being public, and an
/// accidental click cannot be undone once the content has been indexed.
#[arbor_rpc::handler]
async fn garrulus_create_remote_repo(
    state: &GarrulusState,
    name: String,
) -> Result<RemoteConfig, String> {
    // Resolved first, and its guard dropped, because `host_call` blocks on the
    // shell's reply — holding any guard across it is how this process deadlocks.
    let root = state.vault_root()?;

    let created: CreatedRepo = serde_json::from_value(
        state.host_call("__garrulus_create_repo", json!({ "name": name, "provider": null }))?,
    )
    .map_err(|e| format!("__garrulus_create_repo returned something unexpected: {e}"))?;
    eprintln!("garrulus-be: private repository created at {}", created.web_url);

    // A vault created through Garrulus is a plain folder; the repository it is
    // about to push to only exists because of the call above. Both git steps are
    // therefore part of "create a destination", not a separate flow the user
    // could reasonably be asked to run themselves.
    ensure_git_repo(&root)?;
    point_origin_at(&root, &created.clone_url)?;

    let config = RemoteConfig {
        kind:       RemoteKind::Git,
        git_remote: Some(DEFAULT_GIT_REMOTE.to_string()),
        branch:     None,
        folder:     None,
    };
    install_and_persist(state, &root, config.clone()).await?;
    Ok(config)
}

// ── Installing ────────────────────────────────────────────────────────────────

/// Install the destination recorded for a vault that is being opened.
///
/// Best-effort by design, exactly like the watcher next to it: a vault whose
/// mirror drive is not plugged in, or whose git binary has gone missing, still
/// opens — local-only, with a line on stderr. Refusing to open a vault because a
/// *sync destination* could not be built would put the user's notes behind a
/// network problem, which is the opposite of what a local-first product promises.
pub(crate) fn install_stored(state: &GarrulusState, root: &Path) {
    // Clear FIRST, unconditionally, before anything can return early.
    //
    // `set_vault` swaps the vault but leaves the remote slot alone, and a
    // `GitRemote` captures its own vault path at construction — it ignores the
    // root it is handed. So opening a synced vault A and then a local-only vault B
    // would leave A's remote installed, and the next press of the sync button
    // would commit and push **into A** while the user is looking at B. Losing that
    // early return is the whole bug.
    if let Err(e) = state.clear_remote() {
        eprintln!("garrulus-be: could not detach the previous sync destination: {e}");
    }
    probe::forget();

    let Some(config) = stored(root) else { return };
    match build_remote(state, root, &config) {
        Ok(remote) => {
            if let Err(e) = state.set_remote(remote) {
                eprintln!("garrulus-be: could not install the sync destination: {e}");
            }
        }
        Err(e) => eprintln!(
            "garrulus-be: sync destination unusable, the vault opens local-only: {e}"
        ),
    }
}

/// The shared tail of every "this is the destination now" path: build it, record
/// it, install it, and report where the vault stands against it.
async fn install_and_persist(
    state: &GarrulusState,
    root: &Path,
    config: RemoteConfig,
) -> Result<RemoteStatus, String> {
    // Refuse a broken destination where the user typed it, not three calls later
    // inside the factory — the error then names the field, not the failure.
    config.validate()?;
    let remote = build_remote(state, root, &config)?;
    let descriptor = remote.descriptor();
    persist(root, Some(config))?;
    state.set_remote(remote)?;
    // The remembered probe result belongs to the *previous* destination.
    probe::forget();
    Ok(RemoteStatus { descriptor, state: probe_or_offline(state).await })
}

/// Probe the freshly installed remote, degrading a failure to `Offline`.
///
/// The destination is already saved by the time this runs, so a probe that could
/// not reach it is news about the network, not about the configuration — and
/// `Offline` is precisely the state the sync button exists to show.
async fn probe_or_offline(state: &GarrulusState) -> SyncState {
    match sync::probe_state(state).await {
        Ok(probed) => probed,
        Err(e) => {
            eprintln!("garrulus-be: the new sync destination did not answer: {e}");
            SyncState::Offline
        }
    }
}

// ── The registry ──────────────────────────────────────────────────────────────

/// Write (or erase) a vault's destination in the profile registry.
///
/// Delegates rather than re-implementing load → mutate → save: two writers of one
/// file drift, and the error string here IS the seam's contract, so a second
/// phrasing of "no such vault" is a second contract.
fn persist(root: &Path, config: Option<RemoteConfig>) -> Result<(), String> {
    set_vault_remote(&vault_id_for(root), config)
}

/// The destination recorded for `root`, or `None` for a local-only vault.
fn stored(root: &Path) -> Option<RemoteConfig> {
    load_vaults().find_by_path(root).and_then(|entry| entry.remote.clone())
}

// ── Git plumbing (adoption only) ──────────────────────────────────────────────

/// The git binary. Garrulus has no configured git program of its own, so this is
/// `git` on `PATH` — the same fallback `GitCli::from_optional` gives every other
/// caller that has not resolved one.
fn git() -> GitCli {
    GitCli::from_optional(None)
}

/// Make sure the vault is a git repository, so a remote can be added to it.
///
/// A vault Garrulus created is a plain folder; a vault the user pointed at may
/// already be one. Both have to end up with a `.git`, and `git init` on an
/// existing repository is not a no-op worth risking, hence the check.
fn ensure_git_repo(root: &Path) -> Result<(), String> {
    if root.join(".git").exists() {
        return Ok(());
    }
    run_git(root, &["init"])
}

/// Point `origin` at `url`, whether or not it already exists.
///
/// `remote add` fails when the name is taken, and that case is ordinary here: a
/// vault that was pointed at one repository and is now being pointed at a fresh
/// one. Falling through to `set-url` is the whole handling it deserves.
fn point_origin_at(root: &Path, url: &str) -> Result<(), String> {
    if run_git(root, &["remote", "add", DEFAULT_GIT_REMOTE, url]).is_ok() {
        return Ok(());
    }
    run_git(root, &["remote", "set-url", DEFAULT_GIT_REMOTE, url])
}

/// Run one git subcommand in the vault, mapping a non-zero exit to git's own
/// stderr — the error string is the wire contract, and git's wording is more
/// useful to the user than anything this file could invent.
fn run_git(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = git()
        .command()
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("git {}: {}", args.join(" "), stderr.trim()))
}
