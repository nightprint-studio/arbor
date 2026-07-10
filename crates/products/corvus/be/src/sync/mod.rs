//! `sync` domain — mirror corvus workspaces / settings / mod-list / light plugin
//! data to a **private git-provider repo** (see `docs/backend-architecture.md`
//! §9 for the on-disk sources).
//!
//! Model: a small, versioned *bundle* of files is pushed to a private repo the
//! user owns (auto-created if absent). A background **driver** rebuilds the
//! bundle, fingerprints it, and pushes when it differs — debounced by
//! `interval_secs`. Reads (pull) go through the provider too. Everything is
//! host-agnostic through the `GitProvider` trait; GitHub is the only impl with
//! `get_repo_file` / `put_repo_file` today, so [`handlers::sync_enable`] gates on it.
//!
//! What travels: workspaces (repos identified by **remote_url**, never absolute
//! paths), a UI/settings subset + the corvus git config, the installed-mod list
//! (+ enable state), and each plugin's small `global.json`. What does NOT: repo
//! paths, credentials (keyring, re-auth per machine), and heavy `arbor/data`
//! caches/indices.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Serialize;

pub(crate) mod driver;
pub(crate) mod engine;
pub(crate) mod handlers;
pub(crate) mod pull;
pub(crate) mod remote;
pub(crate) mod sources;

/// The branch the bundle lives on. New repos created with `auto_init:false` get
/// their first branch from the first `put_repo_file`, so we pin `main`.
pub(crate) const BRANCH: &str = "main";

/// Bundle schema version — bumped when the file layout changes so a puller can
/// refuse/upgrade an incompatible bundle.
pub(crate) const SCHEMA_VERSION: u32 = 1;

// ── Bundle file paths (fixed set; plugin data is dynamic under `plugins/data/`) ──
pub(crate) const F_MANIFEST: &str = "manifest.json";
pub(crate) const F_WORKSPACES: &str = "workspaces.json";
pub(crate) const F_REPOS: &str = "repos.json";
pub(crate) const F_SETTINGS_PROFILE: &str = "settings/profile.toml";
pub(crate) const F_SETTINGS_CORVUS: &str = "settings/corvus.toml";
pub(crate) const F_MODS: &str = "plugins/list.json";
pub(crate) const PLUGIN_DATA_PREFIX: &str = "plugins/data/";

/// One file in the sync bundle — a repo-relative path and its bytes.
#[derive(Debug, Clone)]
pub(crate) struct BundleFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

// ── Dirty tracking ───────────────────────────────────────────────────────────
// The **fingerprint** is authoritative: the driver compares the freshly-built
// bundle's fingerprint to the last one it pushed, so any change (incl. external
// edits and plugin-side writes) is caught without instrumenting every writer.
// `DIRTY` is a cheap fast-path an in-process writer can flip (`mark_dirty`) to
// let the driver skip the debounce wait for a snappier push after a user action.

static DIRTY: AtomicBool = AtomicBool::new(false);
static LAST_PUSHED_FP: AtomicU64 = AtomicU64::new(0);
static HAVE_PUSHED: AtomicBool = AtomicBool::new(false);

/// Flag that something a writer knows about changed. Cheap; safe to over-call.
pub(crate) fn mark_dirty() {
    DIRTY.store(true, Ordering::Relaxed);
}

/// Take-and-clear the fast-path dirty flag.
pub(crate) fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::Relaxed)
}

/// Record the fingerprint just pushed, so later builds can tell if they differ.
pub(crate) fn record_pushed(fp: u64) {
    LAST_PUSHED_FP.store(fp, Ordering::Relaxed);
    HAVE_PUSHED.store(true, Ordering::Relaxed);
}

/// True when `fp` differs from the last pushed fingerprint (or nothing pushed yet).
pub(crate) fn is_dirty(fp: u64) -> bool {
    !HAVE_PUSHED.load(Ordering::Relaxed) || LAST_PUSHED_FP.load(Ordering::Relaxed) != fp
}

// ── Small shared helpers ───────────────────────────────────────────────────────

/// Unix epoch seconds (best-effort; `0` if the clock is before the epoch).
pub(crate) fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A human-ish machine identifier stamped into the manifest (best-effort).
pub(crate) fn machine_id() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Status snapshot the FE renders (config knobs + live dirty flag).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SyncStatus {
    pub enabled: bool,
    pub provider: Option<String>,
    pub repo_full_name: Option<String>,
    pub clone_url: Option<String>,
    pub interval_secs: u64,
    pub include_workspaces: bool,
    pub include_settings: bool,
    pub include_mods: bool,
    pub include_plugin_data: bool,
    pub last_push_at: Option<i64>,
    pub last_pull_at: Option<i64>,
    pub last_machine: Option<String>,
    /// Whether local state differs from what was last pushed (best-effort;
    /// `false` when sync is disabled or the bundle can't be built yet).
    pub dirty: bool,
    /// Adopted an existing repo — a pull is needed before auto-push resumes.
    pub awaiting_pull: bool,
}
