//! `vault` domain — open / create / list / close a vault.
//!
//! Opening is the one heavy operation in Garrulus: it reads the vault's marker
//! folder and note types, parses every note, and builds the index from scratch.
//! That is deliberate at personal-vault scale (thousands of notes, not the two
//! million symbols bennu-index was built for) — a full rebuild is instant and has
//! a fraction of the bug surface of incremental repair. The index is a cache, and
//! this is the moment it is allowed to cost something.

use std::path::{Path, PathBuf};

use garrulus_core::prelude::{
    hooks, load_config, load_vaults, remember_vault, save_config, save_vaults, GarrulusState,
    Vault, VaultEntry,
};
use serde::Serialize;
use serde_json::json;

use crate::probe;
use crate::remote;
use crate::vault_io;
use crate::watch;

/// What the frontend gets back when a vault opens: enough to title the window and
/// draw the empty state, not the vault's contents (those come from `search` /
/// the file tree).
#[derive(Debug, Clone, Serialize)]
pub struct VaultSummary {
    /// Stable id, also the key of the vault's index cache directory.
    pub id: String,
    /// Absolute vault root.
    pub root: String,
    /// Name shown in the switcher — the folder name unless the user renamed it.
    pub display_name: String,
    /// Notes indexed at open.
    pub note_count: usize,
    /// Note types declared in `<vault>/.arbor/garrulus/types/`.
    pub type_count: usize,
}

/// Open a vault: parse it, build the index, start the watcher, remember it.
#[arbor_rpc::handler]
fn garrulus_open_vault(state: &GarrulusState, path: String) -> Result<VaultSummary, String> {
    let vault = vault_io::open_vault(&PathBuf::from(&path))?;
    install(state, vault, None)
}

/// Create a vault at `path` (marker folder, default settings, built-in types) and
/// open it. Fails when one is already there — re-opening is `garrulus_open_vault`.
#[arbor_rpc::handler]
fn garrulus_create_vault(
    state: &GarrulusState,
    path: String,
    display_name: Option<String>,
) -> Result<VaultSummary, String> {
    // The name goes to `Vault::create` as well as to the registry: it is what
    // lands in the vault's own `vault.toml`, so it travels to the other machine.
    // Empty means "use the folder's name", which the vault crate handles.
    let vault = vault_io::create_vault(
        &PathBuf::from(&path),
        display_name.as_deref().unwrap_or_default(),
    )?;
    install(state, vault, display_name)
}

/// Every vault this profile knows about, most recently opened first.
#[arbor_rpc::handler]
fn garrulus_list_vaults(_state: &GarrulusState) -> Result<Vec<VaultEntry>, String> {
    Ok(load_vaults().vaults)
}

/// Close the open vault: stop the watcher, drop the vault, empty the index and
/// detach the remote. A no-op when nothing is open.
#[arbor_rpc::handler]
fn garrulus_close_vault(state: &GarrulusState) -> Result<(), String> {
    watch::stop();
    let Some(root) = state.close_vault()? else { return Ok(()) };
    // The probe's remembered state and the last pull's conflicts both belong to
    // the vault that just closed — carried forward, they would describe vault A
    // while the user is looking at vault B.
    probe::forget();
    crate::sync::forget_all_conflicts();
    state.fire_hook(hooks::VAULT_CLOSED, json!({ "path": root.to_string_lossy() }));
    Ok(())
}

/// Re-read every note and rebuild the index from scratch, returning how many
/// notes are in it.
///
/// The escape hatch for the one thing an incremental cache cannot promise: that it
/// never drifted. Something changed the vault without the watcher seeing it (a
/// network share, a `git checkout` in a terminal, Obsidian mid-crash), and the
/// cheapest honest answer at personal-vault scale is to throw the cache away — the
/// same rebuild a vault open does, which is instant.
#[arbor_rpc::handler]
fn garrulus_rebuild_index(state: &GarrulusState) -> Result<usize, String> {
    let notes = vault_io::with_vault(state, vault_io::scan_notes)?;
    let note_count = notes.len();
    state.rebuild_index(notes)?;
    Ok(note_count)
}

/// The shared tail of open and create: install the vault + a freshly built index,
/// start the watcher, record the vault in the registry and in `last_vault`, then
/// fire `garrulus:vault_opened` with every lock already dropped.
fn install(
    state: &GarrulusState,
    vault: Vault,
    display_name: Option<String>,
) -> Result<VaultSummary, String> {
    let root = vault.root.clone();
    let type_count = vault.types.len();
    let notes = vault_io::scan_notes(&vault)?;
    let note_count = notes.len();

    state.set_vault(vault)?;
    state.rebuild_index(notes)?;

    let mut entry = remember_vault(&root, Some(vault_io::now_ms()))?;
    if let Some(name) = display_name.filter(|n| !n.trim().is_empty()) {
        entry = rename_in_registry(&entry.id, name.trim())?;
    }

    // The watcher is best-effort: a vault on a filesystem `notify` cannot watch
    // (some network shares) is still perfectly usable — the user just has to
    // refresh by hand — so a failure is a log line, not a failed open.
    let debounce = load_config().watch_debounce_ms;
    if let Err(e) = watch::start(state.event_sink(), root.clone(), debounce) {
        eprintln!("garrulus-be: vault watcher unavailable: {e}");
    }
    // The vault's sync destination lives in the registry entry, so re-opening a
    // vault has to re-install it — without this the destination would silently
    // survive on disk and be gone in the running process, and every sync handler
    // would answer `NoRemote` for a vault the user had configured. Best-effort,
    // the same posture as the watcher above: a remote that will not build is a
    // local-only vault, never a failed open.
    remote::install_stored(state, &root);
    // The conflict list is a process global, so opening a vault over another one
    // has to clear it here too — the frontend opens B without closing A, which is
    // the normal path, and `garrulus_resolve_conflict` would then act on B's root
    // with a side-file path that only ever existed in A.
    crate::sync::forget_all_conflicts();
    remember_last_vault(&root);

    let summary = VaultSummary {
        id: entry.id,
        root: root.to_string_lossy().to_string(),
        display_name: entry.display_name,
        note_count,
        type_count,
    };
    // No lock held: `set_vault` / `rebuild_index` dropped their guards inside.
    state.fire_hook(
        hooks::VAULT_OPENED,
        json!({
            "vault_id":   summary.id,
            "path":       summary.root,
            "name":       summary.display_name,
            "note_count": summary.note_count,
        }),
    );
    Ok(summary)
}

/// Give a registry entry a display name of the user's choosing.
fn rename_in_registry(id: &str, name: &str) -> Result<VaultEntry, String> {
    let mut reg = load_vaults();
    let entry = reg
        .vaults
        .iter_mut()
        .find(|v| v.id == id)
        .ok_or_else(|| format!("vault {id} is not in the registry"))?;
    entry.display_name = name.to_string();
    let updated = entry.clone();
    save_vaults(&reg)?;
    Ok(updated)
}

/// Remember which vault to re-open at startup. Best-effort by design: failing to
/// write a preference must never fail an open the user already got.
fn remember_last_vault(root: &Path) {
    let mut cfg = load_config();
    let path = root.to_string_lossy().to_string();
    if cfg.last_vault == path {
        return;
    }
    cfg.last_vault = path;
    if let Err(e) = save_config(&cfg) {
        eprintln!("garrulus-be: could not record the last vault: {e}");
    }
}
